//! Pure popup geometry for the macOS global desktop coordinate space.
//!
//! CoreGraphics selection anchors are expressed in global logical points while
//! Tauri positions popup windows in global physical pixels. Mixed-scale monitor
//! layouts cannot be converted with one desktop-wide scale factor, so every
//! conversion is relative to the monitor that contains the point and uses both
//! that monitor's logical and physical origins.

use std::fmt;

/// A point in the CoreGraphics global logical coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalPoint {
    pub x: f64,
    pub y: f64,
}

/// A work-area rectangle in the CoreGraphics global logical coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalRect {
    pub origin: LogicalPoint,
    pub width: f64,
    pub height: f64,
}

/// A point in the window manager's global physical coordinate space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalPoint {
    pub x: i32,
    pub y: i32,
}

/// A size measured in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalSize {
    pub width: u32,
    pub height: u32,
}

/// A rectangle in the window manager's global physical coordinate space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalRect {
    pub origin: PhysicalPoint,
    pub size: PhysicalSize,
}

/// The geometry needed to map one monitor between logical and physical space.
///
/// `physical_origin` is intentionally explicit. Deriving it by multiplying the
/// logical origin fails when adjacent monitors use different scale factors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonitorGeometry {
    pub logical_origin: LogicalPoint,
    pub physical_origin: PhysicalPoint,
    pub physical_size: PhysicalSize,
    /// Usable bounds after excluding system UI such as the menu bar and Dock.
    /// This uses the same global physical coordinate space as `physical_origin`.
    pub physical_work_area: PhysicalRect,
    pub scale_factor: f64,
}

/// Inputs for placing a popup beside a selection anchor.
///
/// The preferred placement is below and to the right of the anchor. An axis is
/// flipped to the opposite side when the preferred side does not fit. If
/// neither side fits, it is clamped to the monitor's padded physical bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupPlacement {
    pub anchor: LogicalPoint,
    pub monitor: MonitorGeometry,
    pub popup_size: PhysicalSize,
    pub offset_logical: f64,
    pub edge_padding_logical: f64,
}

/// Inputs for hit-testing a CG global logical point against a physical popup.
///
/// `point_monitor` must be the monitor containing `point`. `popup_monitor` is
/// used only to scale the logical hit-test padding. Keeping these separate makes
/// outside-click tests correct even when the click and popup are on monitors
/// with different scale factors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupHitTest {
    pub point: LogicalPoint,
    pub point_monitor: MonitorGeometry,
    pub popup: PhysicalRect,
    pub popup_monitor: MonitorGeometry,
    pub padding_logical: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryError {
    NonFiniteCoordinate,
    InvalidScaleFactor,
    InvalidSpacing,
    EmptyMonitor,
    InvalidWorkArea,
    EmptyPopup,
    CoordinateOutOfRange,
}

impl fmt::Display for GeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NonFiniteCoordinate => "geometry contains a non-finite logical coordinate",
            Self::InvalidScaleFactor => "monitor scale factor must be finite and greater than zero",
            Self::InvalidSpacing => "popup offset and padding must be finite and non-negative",
            Self::EmptyMonitor => "monitor physical size must be non-zero",
            Self::InvalidWorkArea => {
                "monitor work area must be non-empty and contained by monitor bounds"
            }
            Self::EmptyPopup => "popup physical size must be non-zero",
            Self::CoordinateOutOfRange => "physical coordinate is outside the supported i32 range",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for GeometryError {}

/// Convert a CG global logical point to a global physical position.
///
/// The conversion is monitor-relative, which preserves negative origins and
/// remains valid for monitors above, below, or beside the primary display.
pub fn cg_global_logical_to_physical(
    point: LogicalPoint,
    monitor: MonitorGeometry,
) -> Result<PhysicalPoint, GeometryError> {
    validate_monitor(monitor)?;
    validate_logical_point(point)?;

    let x = monitor.physical_origin.x as f64
        + (point.x - monitor.logical_origin.x) * monitor.scale_factor;
    let y = monitor.physical_origin.y as f64
        + (point.y - monitor.logical_origin.y) * monitor.scale_factor;

    Ok(PhysicalPoint {
        x: rounded_i32(x)?,
        y: rounded_i32(y)?,
    })
}

/// Map a native CoreGraphics logical work area into Tauri's global physical space.
///
/// The mapping is monitor-relative. In particular, a negative logical origin or a
/// Retina scale factor must never be multiplied as if the whole desktop shared one
/// scale. The returned rectangle is rejected unless it remains fully inside the
/// supplied physical monitor bounds.
pub fn logical_work_area_to_physical(
    logical_monitor_origin: LogicalPoint,
    physical_monitor_origin: PhysicalPoint,
    physical_monitor_size: PhysicalSize,
    scale_factor: f64,
    logical_work_area: LogicalRect,
) -> Result<PhysicalRect, GeometryError> {
    validate_logical_point(logical_monitor_origin)?;
    validate_logical_point(logical_work_area.origin)?;
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return Err(GeometryError::InvalidScaleFactor);
    }
    if physical_monitor_size.width == 0 || physical_monitor_size.height == 0 {
        return Err(GeometryError::EmptyMonitor);
    }
    if !logical_work_area.width.is_finite()
        || !logical_work_area.height.is_finite()
        || logical_work_area.width <= 0.0
        || logical_work_area.height <= 0.0
    {
        return Err(GeometryError::InvalidWorkArea);
    }

    let origin_x = physical_monitor_origin.x as f64
        + (logical_work_area.origin.x - logical_monitor_origin.x) * scale_factor;
    let origin_y = physical_monitor_origin.y as f64
        + (logical_work_area.origin.y - logical_monitor_origin.y) * scale_factor;
    let area = PhysicalRect {
        origin: PhysicalPoint {
            x: rounded_i32(origin_x)?,
            y: rounded_i32(origin_y)?,
        },
        size: PhysicalSize {
            width: rounded_positive_u32(logical_work_area.width * scale_factor)?,
            height: rounded_positive_u32(logical_work_area.height * scale_factor)?,
        },
    };
    let monitor = MonitorGeometry {
        logical_origin: logical_monitor_origin,
        physical_origin: physical_monitor_origin,
        physical_size: physical_monitor_size,
        physical_work_area: area,
        scale_factor,
    };
    validate_work_area(monitor)?;
    Ok(area)
}

/// Place a popup within the physical work area of its anchor's monitor.
///
/// Negative global coordinates are retained. When the popup is larger than the
/// padded available area, it is aligned to the leading padded edge; complete
/// containment is impossible in that case, but the popup's leading controls
/// remain reachable and the calculation stays deterministic.
pub fn place_popup(input: PopupPlacement) -> Result<PhysicalPoint, GeometryError> {
    validate_monitor(input.monitor)?;
    validate_popup_size(input.popup_size)?;
    validate_spacing(input.offset_logical)?;
    validate_spacing(input.edge_padding_logical)?;

    let anchor = cg_global_logical_to_physical(input.anchor, input.monitor)?;
    let offset = logical_distance_to_physical(input.offset_logical, input.monitor.scale_factor)?;
    let padding =
        logical_distance_to_physical(input.edge_padding_logical, input.monitor.scale_factor)?;

    let work_area = input.monitor.physical_work_area;
    let x = place_axis(
        i64::from(anchor.x),
        i64::from(work_area.origin.x),
        i64::from(work_area.size.width),
        i64::from(input.popup_size.width),
        offset,
        padding,
    );
    let y = place_axis(
        i64::from(anchor.y),
        i64::from(work_area.origin.y),
        i64::from(work_area.size.height),
        i64::from(input.popup_size.height),
        offset,
        padding,
    );

    Ok(PhysicalPoint {
        x: i32::try_from(x).map_err(|_| GeometryError::CoordinateOutOfRange)?,
        y: i32::try_from(y).map_err(|_| GeometryError::CoordinateOutOfRange)?,
    })
}

/// Hit-test a CG global logical point against a popup in physical coordinates.
///
/// This calls [`cg_global_logical_to_physical`], the same conversion used by
/// [`place_popup`], so placement and outside-click handling cannot silently use
/// different coordinate assumptions.
pub fn popup_contains_point(input: PopupHitTest) -> Result<bool, GeometryError> {
    validate_monitor(input.point_monitor)?;
    validate_monitor(input.popup_monitor)?;
    validate_popup_size(input.popup.size)?;
    validate_spacing(input.padding_logical)?;

    let point = cg_global_logical_to_physical(input.point, input.point_monitor)?;
    let padding =
        logical_distance_to_physical(input.padding_logical, input.popup_monitor.scale_factor)?;
    let left = i64::from(input.popup.origin.x) - padding;
    let top = i64::from(input.popup.origin.y) - padding;
    let right = i64::from(input.popup.origin.x) + i64::from(input.popup.size.width) + padding;
    let bottom = i64::from(input.popup.origin.y) + i64::from(input.popup.size.height) + padding;

    Ok(i64::from(point.x) >= left
        && i64::from(point.x) <= right
        && i64::from(point.y) >= top
        && i64::from(point.y) <= bottom)
}

fn place_axis(
    anchor: i64,
    monitor_origin: i64,
    monitor_extent: i64,
    popup_extent: i64,
    offset: i64,
    padding: i64,
) -> i64 {
    // A caller-provided padding larger than half the monitor cannot be
    // satisfied. Cap it to a representable interior instead of allowing the
    // leading edge itself to move beyond the monitor.
    let padding = padding.min((monitor_extent - 1) / 2);
    let available_start = monitor_origin + padding;
    let available_end = monitor_origin + monitor_extent - padding;

    // The popup cannot fit. Aligning to the leading edge is predictable and
    // keeps title/close controls accessible instead of producing an invalid
    // clamp interval.
    if popup_extent >= available_end - available_start {
        return available_start;
    }

    let latest_start = available_end - popup_extent;
    let preferred = anchor + offset;
    if (available_start..=latest_start).contains(&preferred) {
        return preferred;
    }

    let flipped = anchor - popup_extent - offset;
    if (available_start..=latest_start).contains(&flipped) {
        return flipped;
    }

    preferred.clamp(available_start, latest_start)
}

fn validate_monitor(monitor: MonitorGeometry) -> Result<(), GeometryError> {
    validate_logical_point(monitor.logical_origin)?;
    if !monitor.scale_factor.is_finite() || monitor.scale_factor <= 0.0 {
        return Err(GeometryError::InvalidScaleFactor);
    }
    if monitor.physical_size.width == 0 || monitor.physical_size.height == 0 {
        return Err(GeometryError::EmptyMonitor);
    }
    validate_work_area(monitor)?;
    Ok(())
}

fn validate_work_area(monitor: MonitorGeometry) -> Result<(), GeometryError> {
    let area = monitor.physical_work_area;
    if area.size.width == 0 || area.size.height == 0 {
        return Err(GeometryError::InvalidWorkArea);
    }

    let monitor_left = i64::from(monitor.physical_origin.x);
    let monitor_top = i64::from(monitor.physical_origin.y);
    let monitor_right = monitor_left + i64::from(monitor.physical_size.width);
    let monitor_bottom = monitor_top + i64::from(monitor.physical_size.height);
    let area_left = i64::from(area.origin.x);
    let area_top = i64::from(area.origin.y);
    let area_right = area_left + i64::from(area.size.width);
    let area_bottom = area_top + i64::from(area.size.height);

    if area_left < monitor_left
        || area_top < monitor_top
        || area_right > monitor_right
        || area_bottom > monitor_bottom
    {
        return Err(GeometryError::InvalidWorkArea);
    }
    Ok(())
}

fn validate_popup_size(size: PhysicalSize) -> Result<(), GeometryError> {
    if size.width == 0 || size.height == 0 {
        return Err(GeometryError::EmptyPopup);
    }
    Ok(())
}

fn validate_logical_point(point: LogicalPoint) -> Result<(), GeometryError> {
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err(GeometryError::NonFiniteCoordinate);
    }
    Ok(())
}

fn validate_spacing(value: f64) -> Result<(), GeometryError> {
    if !value.is_finite() || value < 0.0 {
        return Err(GeometryError::InvalidSpacing);
    }
    Ok(())
}

fn logical_distance_to_physical(value: f64, scale_factor: f64) -> Result<i64, GeometryError> {
    let scaled = value * scale_factor;
    if !scaled.is_finite() || scaled.round() > i32::MAX as f64 {
        return Err(GeometryError::CoordinateOutOfRange);
    }
    Ok(scaled.round() as i64)
}

fn rounded_i32(value: f64) -> Result<i32, GeometryError> {
    if !value.is_finite() {
        return Err(GeometryError::CoordinateOutOfRange);
    }
    let rounded = value.round();
    if rounded < i32::MIN as f64 || rounded > i32::MAX as f64 {
        return Err(GeometryError::CoordinateOutOfRange);
    }
    Ok(rounded as i32)
}

fn rounded_positive_u32(value: f64) -> Result<u32, GeometryError> {
    if !value.is_finite() {
        return Err(GeometryError::InvalidWorkArea);
    }
    let rounded = value.round();
    if rounded < 1.0 || rounded > u32::MAX as f64 {
        return Err(GeometryError::InvalidWorkArea);
    }
    Ok(rounded as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const POPUP: PhysicalSize = PhysicalSize {
        width: 320,
        height: 100,
    };

    fn monitor(
        logical_x: f64,
        logical_y: f64,
        physical_x: i32,
        physical_y: i32,
        width: u32,
        height: u32,
        scale_factor: f64,
    ) -> MonitorGeometry {
        MonitorGeometry {
            logical_origin: LogicalPoint {
                x: logical_x,
                y: logical_y,
            },
            physical_origin: PhysicalPoint {
                x: physical_x,
                y: physical_y,
            },
            physical_size: PhysicalSize { width, height },
            physical_work_area: PhysicalRect {
                origin: PhysicalPoint {
                    x: physical_x,
                    y: physical_y,
                },
                size: PhysicalSize { width, height },
            },
            scale_factor,
        }
    }

    fn with_work_area(monitor: MonitorGeometry, work_area: PhysicalRect) -> MonitorGeometry {
        MonitorGeometry {
            physical_work_area: work_area,
            ..monitor
        }
    }

    fn placement(anchor: LogicalPoint, monitor: MonitorGeometry) -> PopupPlacement {
        PopupPlacement {
            anchor,
            monitor,
            popup_size: POPUP,
            offset_logical: 8.0,
            edge_padding_logical: 12.0,
        }
    }

    #[test]
    fn main_monitor_clamps_at_left_edge() {
        let result = place_popup(placement(
            LogicalPoint { x: 1.0, y: 200.0 },
            monitor(0.0, 0.0, 0, 0, 1920, 1080, 1.0),
        ));

        assert_eq!(result, Ok(PhysicalPoint { x: 12, y: 208 }));
    }

    #[test]
    fn main_monitor_clamps_at_top_edge() {
        let result = place_popup(placement(
            LogicalPoint { x: 100.0, y: 1.0 },
            monitor(0.0, 0.0, 0, 0, 1920, 1080, 1.0),
        ));

        assert_eq!(result, Ok(PhysicalPoint { x: 108, y: 12 }));
    }

    #[test]
    fn main_monitor_flips_at_right_edge() {
        let result = place_popup(placement(
            LogicalPoint {
                x: 1900.0,
                y: 200.0,
            },
            monitor(0.0, 0.0, 0, 0, 1920, 1080, 1.0),
        ));

        assert_eq!(result, Ok(PhysicalPoint { x: 1572, y: 208 }));
    }

    #[test]
    fn main_monitor_flips_at_bottom_edge() {
        let result = place_popup(placement(
            LogicalPoint {
                x: 400.0,
                y: 1060.0,
            },
            monitor(0.0, 0.0, 0, 0, 1920, 1080, 1.0),
        ));

        assert_eq!(result, Ok(PhysicalPoint { x: 408, y: 952 }));
    }

    #[test]
    fn bottom_right_corner_flips_both_axes() {
        let result = place_popup(placement(
            LogicalPoint {
                x: 1900.0,
                y: 1060.0,
            },
            monitor(0.0, 0.0, 0, 0, 1920, 1080, 1.0),
        ));

        assert_eq!(result, Ok(PhysicalPoint { x: 1572, y: 952 }));
    }

    #[test]
    fn menu_bar_and_dock_insets_bound_popup_to_visible_work_area() {
        let geometry = with_work_area(
            monitor(0.0, 0.0, 0, 0, 1920, 1080, 1.0),
            PhysicalRect {
                origin: PhysicalPoint { x: 0, y: 48 },
                size: PhysicalSize {
                    width: 1820,
                    height: 932,
                },
            },
        );

        assert_eq!(
            place_popup(placement(LogicalPoint { x: 1.0, y: 1.0 }, geometry)),
            Ok(PhysicalPoint { x: 12, y: 60 })
        );
        assert_eq!(
            place_popup(placement(
                LogicalPoint {
                    x: 1900.0,
                    y: 1060.0,
                },
                geometry,
            )),
            Ok(PhysicalPoint { x: 1488, y: 868 })
        );
    }

    #[test]
    fn negative_origin_monitor_uses_its_inset_work_area() {
        let geometry = with_work_area(
            monitor(-1280.0, -900.0, -1280, -1800, 1280, 1800, 2.0),
            PhysicalRect {
                origin: PhysicalPoint { x: -1240, y: -1748 },
                size: PhysicalSize {
                    width: 1200,
                    height: 1650,
                },
            },
        );

        assert_eq!(
            place_popup(PopupPlacement {
                anchor: LogicalPoint {
                    x: -1279.0,
                    y: -899.0,
                },
                monitor: geometry,
                popup_size: PhysicalSize {
                    width: 640,
                    height: 200,
                },
                offset_logical: 8.0,
                edge_padding_logical: 12.0,
            }),
            Ok(PhysicalPoint { x: -1216, y: -1724 })
        );
    }

    #[test]
    fn left_monitor_preserves_negative_x() {
        let result = place_popup(placement(
            LogicalPoint {
                x: -1279.0,
                y: 200.0,
            },
            monitor(-1280.0, 0.0, -1280, 0, 1280, 1024, 1.0),
        ));

        assert_eq!(result, Ok(PhysicalPoint { x: -1268, y: 208 }));
    }

    #[test]
    fn upper_monitor_preserves_negative_y() {
        let result = place_popup(placement(
            LogicalPoint {
                x: 300.0,
                y: -850.0,
            },
            monitor(0.0, -900.0, 0, -900, 1440, 900, 1.0),
        ));

        assert_eq!(result, Ok(PhysicalPoint { x: 308, y: -842 }));
    }

    #[test]
    fn lower_monitor_uses_its_positive_origin() {
        let result = place_popup(placement(
            LogicalPoint {
                x: 400.0,
                y: 1200.0,
            },
            monitor(0.0, 1080.0, 0, 1080, 1920, 1080, 1.0),
        ));

        assert_eq!(result, Ok(PhysicalPoint { x: 408, y: 1208 }));
    }

    #[test]
    fn retina_monitor_scales_anchor_offset_and_padding() {
        let result = place_popup(PopupPlacement {
            anchor: LogicalPoint { x: 100.0, y: 100.0 },
            monitor: monitor(0.0, 0.0, 0, 0, 2880, 1800, 2.0),
            popup_size: PhysicalSize {
                width: 640,
                height: 200,
            },
            offset_logical: 8.0,
            edge_padding_logical: 12.0,
        });

        assert_eq!(result, Ok(PhysicalPoint { x: 216, y: 216 }));
    }

    #[test]
    fn mixed_scale_monitor_uses_both_explicit_origins() {
        let result = place_popup(PopupPlacement {
            anchor: LogicalPoint {
                x: 1500.0,
                y: 200.0,
            },
            monitor: monitor(1440.0, 100.0, 2880, 100, 2560, 1440, 1.25),
            popup_size: PhysicalSize {
                width: 400,
                height: 125,
            },
            offset_logical: 7.2,
            edge_padding_logical: 10.0,
        });

        // Local logical anchor (60, 100) maps to physical (2955, 225), then
        // the 7.2-point offset maps to exactly 9 physical pixels.
        assert_eq!(result, Ok(PhysicalPoint { x: 2964, y: 234 }));
    }

    #[test]
    fn non_integer_scale_rounds_once_in_physical_space() {
        let geometry = monitor(-1000.0, -600.0, -1500, -900, 1500, 900, 1.5);

        assert_eq!(
            cg_global_logical_to_physical(
                LogicalPoint {
                    x: -900.0,
                    y: -500.0,
                },
                geometry,
            ),
            Ok(PhysicalPoint { x: -1350, y: -750 })
        );
        assert_eq!(
            place_popup(PopupPlacement {
                anchor: LogicalPoint {
                    x: -900.0,
                    y: -500.0,
                },
                monitor: geometry,
                popup_size: PhysicalSize {
                    width: 480,
                    height: 150,
                },
                offset_logical: 7.0,
                edge_padding_logical: 10.0,
            }),
            Ok(PhysicalPoint { x: -1339, y: -739 })
        );
    }

    #[test]
    fn popup_larger_than_available_area_aligns_to_leading_padding() {
        let result = place_popup(PopupPlacement {
            anchor: LogicalPoint { x: 400.0, y: 300.0 },
            monitor: monitor(0.0, 0.0, 0, 0, 800, 600, 1.0),
            popup_size: PhysicalSize {
                width: 900,
                height: 700,
            },
            offset_logical: 8.0,
            edge_padding_logical: 20.0,
        });

        assert_eq!(result, Ok(PhysicalPoint { x: 20, y: 20 }));
    }

    #[test]
    fn excessive_padding_cannot_move_popup_origin_beyond_monitor() {
        let result = place_popup(PopupPlacement {
            anchor: LogicalPoint { x: 400.0, y: 300.0 },
            monitor: monitor(-800.0, -600.0, -800, -600, 800, 600, 1.0),
            popup_size: POPUP,
            offset_logical: 8.0,
            edge_padding_logical: 10_000.0,
        });

        assert_eq!(result, Ok(PhysicalPoint { x: -401, y: -301 }));
    }

    #[test]
    fn hit_test_scales_padding_and_includes_its_boundary() {
        let retina = monitor(0.0, 0.0, 0, 0, 2880, 1800, 2.0);
        let popup = PhysicalRect {
            origin: PhysicalPoint { x: 200, y: 200 },
            size: PhysicalSize {
                width: 400,
                height: 100,
            },
        };

        assert_eq!(
            popup_contains_point(PopupHitTest {
                point: LogicalPoint { x: 96.0, y: 100.0 },
                point_monitor: retina,
                popup,
                popup_monitor: retina,
                padding_logical: 4.0,
            }),
            Ok(true)
        );
        assert_eq!(
            popup_contains_point(PopupHitTest {
                point: LogicalPoint { x: 95.5, y: 100.0 },
                point_monitor: retina,
                popup,
                popup_monitor: retina,
                padding_logical: 4.0,
            }),
            Ok(false)
        );
        assert_eq!(
            popup_contains_point(PopupHitTest {
                point: LogicalPoint { x: 304.0, y: 150.0 },
                point_monitor: retina,
                popup,
                popup_monitor: retina,
                padding_logical: 4.0,
            }),
            Ok(true)
        );
        assert_eq!(
            popup_contains_point(PopupHitTest {
                point: LogicalPoint { x: 304.5, y: 150.0 },
                point_monitor: retina,
                popup,
                popup_monitor: retina,
                padding_logical: 4.0,
            }),
            Ok(false)
        );
    }

    #[test]
    fn hit_test_converts_click_with_its_own_monitor_scale() {
        let popup_monitor = monitor(0.0, 0.0, 0, 0, 2880, 1800, 2.0);
        let click_monitor = monitor(-1280.0, 0.0, -1280, 0, 1280, 1024, 1.0);
        let popup = PhysicalRect {
            origin: PhysicalPoint { x: 200, y: 200 },
            size: PhysicalSize {
                width: 400,
                height: 100,
            },
        };

        assert_eq!(
            popup_contains_point(PopupHitTest {
                point: LogicalPoint {
                    x: -1000.0,
                    y: 240.0,
                },
                point_monitor: click_monitor,
                popup,
                popup_monitor,
                padding_logical: 12.0,
            }),
            Ok(false)
        );
    }

    #[test]
    fn native_logical_work_area_maps_menu_and_bottom_dock_at_retina_scale() {
        assert_eq!(
            logical_work_area_to_physical(
                LogicalPoint { x: 0.0, y: 0.0 },
                PhysicalPoint { x: 0, y: 0 },
                PhysicalSize {
                    width: 2880,
                    height: 1800,
                },
                2.0,
                LogicalRect {
                    origin: LogicalPoint { x: 0.0, y: 25.0 },
                    width: 1440.0,
                    height: 805.0,
                },
            ),
            Ok(PhysicalRect {
                origin: PhysicalPoint { x: 0, y: 50 },
                size: PhysicalSize {
                    width: 2880,
                    height: 1610,
                },
            })
        );
    }

    #[test]
    fn native_logical_work_area_preserves_negative_and_explicit_mixed_scale_origins() {
        assert_eq!(
            logical_work_area_to_physical(
                LogicalPoint { x: -1440.0, y: 0.0 },
                PhysicalPoint { x: -1440, y: 0 },
                PhysicalSize {
                    width: 1440,
                    height: 900,
                },
                1.0,
                LogicalRect {
                    origin: LogicalPoint {
                        x: -1360.0,
                        y: 25.0
                    },
                    width: 1360.0,
                    height: 875.0,
                },
            ),
            Ok(PhysicalRect {
                origin: PhysicalPoint { x: -1360, y: 25 },
                size: PhysicalSize {
                    width: 1360,
                    height: 875,
                },
            })
        );

        assert_eq!(
            logical_work_area_to_physical(
                LogicalPoint {
                    x: 1440.0,
                    y: 100.0,
                },
                PhysicalPoint { x: 2880, y: 100 },
                PhysicalSize {
                    width: 2560,
                    height: 1440,
                },
                1.25,
                LogicalRect {
                    origin: LogicalPoint {
                        x: 1448.0,
                        y: 124.0,
                    },
                    width: 2032.0,
                    height: 1120.0,
                },
            ),
            Ok(PhysicalRect {
                origin: PhysicalPoint { x: 2890, y: 130 },
                size: PhysicalSize {
                    width: 2540,
                    height: 1400,
                },
            })
        );
    }

    #[test]
    fn native_logical_work_area_rejects_missing_or_out_of_monitor_geometry() {
        let arguments = (
            LogicalPoint { x: 0.0, y: 0.0 },
            PhysicalPoint { x: 0, y: 0 },
            PhysicalSize {
                width: 1440,
                height: 900,
            },
            1.0,
        );
        assert_eq!(
            logical_work_area_to_physical(
                arguments.0,
                arguments.1,
                arguments.2,
                arguments.3,
                LogicalRect {
                    origin: LogicalPoint { x: 0.0, y: 0.0 },
                    width: 0.0,
                    height: 875.0,
                },
            ),
            Err(GeometryError::InvalidWorkArea)
        );
        assert_eq!(
            logical_work_area_to_physical(
                arguments.0,
                arguments.1,
                arguments.2,
                arguments.3,
                LogicalRect {
                    origin: LogicalPoint { x: -1.0, y: 25.0 },
                    width: 1440.0,
                    height: 875.0,
                },
            ),
            Err(GeometryError::InvalidWorkArea)
        );
        assert_eq!(
            logical_work_area_to_physical(
                arguments.0,
                arguments.1,
                arguments.2,
                arguments.3,
                LogicalRect {
                    origin: LogicalPoint { x: 0.0, y: 25.0 },
                    width: f64::NAN,
                    height: 875.0,
                },
            ),
            Err(GeometryError::InvalidWorkArea)
        );
    }

    #[test]
    fn rejects_invalid_scale_spacing_and_empty_geometry() {
        let valid_monitor = monitor(0.0, 0.0, 0, 0, 1920, 1080, 1.0);
        let mut invalid_scale = valid_monitor;
        invalid_scale.scale_factor = 0.0;

        assert_eq!(
            place_popup(placement(LogicalPoint { x: 10.0, y: 10.0 }, invalid_scale)),
            Err(GeometryError::InvalidScaleFactor)
        );
        assert_eq!(
            place_popup(PopupPlacement {
                edge_padding_logical: -1.0,
                ..placement(LogicalPoint { x: 10.0, y: 10.0 }, valid_monitor,)
            }),
            Err(GeometryError::InvalidSpacing)
        );
        assert_eq!(
            place_popup(PopupPlacement {
                popup_size: PhysicalSize {
                    width: 0,
                    height: 100,
                },
                ..placement(LogicalPoint { x: 10.0, y: 10.0 }, valid_monitor,)
            }),
            Err(GeometryError::EmptyPopup)
        );

        let invalid_work_area = with_work_area(
            valid_monitor,
            PhysicalRect {
                origin: PhysicalPoint { x: -1, y: 0 },
                size: PhysicalSize {
                    width: 1920,
                    height: 1080,
                },
            },
        );
        assert_eq!(
            place_popup(placement(
                LogicalPoint { x: 10.0, y: 10.0 },
                invalid_work_area,
            )),
            Err(GeometryError::InvalidWorkArea)
        );
    }
}
