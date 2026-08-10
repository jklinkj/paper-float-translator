//! Verified macOS visible-work-area adapter.
//!
//! The Objective-C bridge returns `NSScreen.frame/visibleFrame` after converting
//! their local insets into CoreGraphics' top-left global logical coordinate
//! space. Keep validation at the FFI boundary so popup geometry never consumes
//! a partial, version-skewed, or non-finite platform record.

use crate::popup_geometry::{LogicalPoint, LogicalRect};

const NATIVE_VISIBLE_WORK_AREA_VERSION: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct NativeVisibleWorkArea {
    version: u32,
    display_id: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn paper_float_visible_work_area_for_anchor(
        anchor_x: f64,
        anchor_y: f64,
        work_area: *mut NativeVisibleWorkArea,
    ) -> std::os::raw::c_int;
}

#[cfg(target_os = "macos")]
pub(crate) fn visible_work_area_for_anchor(anchor: LogicalPoint) -> Option<LogicalRect> {
    if !anchor.x.is_finite() || !anchor.y.is_finite() {
        return None;
    }
    let mut record = NativeVisibleWorkArea::default();
    let copied =
        unsafe { paper_float_visible_work_area_for_anchor(anchor.x, anchor.y, &mut record) };
    if copied != 1 {
        return None;
    }
    validate_record(record)
}

fn validate_record(record: NativeVisibleWorkArea) -> Option<LogicalRect> {
    if record.version != NATIVE_VISIBLE_WORK_AREA_VERSION
        || record.display_id == 0
        || !record.x.is_finite()
        || !record.y.is_finite()
        || !record.width.is_finite()
        || !record.height.is_finite()
        || record.width <= 0.0
        || record.height <= 0.0
    {
        return None;
    }
    Some(LogicalRect {
        origin: LogicalPoint {
            x: record.x,
            y: record.y,
        },
        width: record.width,
        height: record.height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_record() -> NativeVisibleWorkArea {
        NativeVisibleWorkArea {
            version: NATIVE_VISIBLE_WORK_AREA_VERSION,
            display_id: 9,
            x: -1440.0,
            y: -24.0,
            width: 1440.0,
            height: 876.0,
        }
    }

    #[test]
    fn accepts_only_the_versioned_finite_positive_record() {
        assert_eq!(
            validate_record(valid_record()),
            Some(LogicalRect {
                origin: LogicalPoint {
                    x: -1440.0,
                    y: -24.0,
                },
                width: 1440.0,
                height: 876.0,
            })
        );
    }

    #[test]
    fn rejects_version_identity_and_extent_failures() {
        for invalid in [
            NativeVisibleWorkArea {
                version: 2,
                ..valid_record()
            },
            NativeVisibleWorkArea {
                display_id: 0,
                ..valid_record()
            },
            NativeVisibleWorkArea {
                width: 0.0,
                ..valid_record()
            },
            NativeVisibleWorkArea {
                height: -1.0,
                ..valid_record()
            },
        ] {
            assert_eq!(validate_record(invalid), None);
        }
    }

    #[test]
    fn rejects_every_non_finite_coordinate_or_extent() {
        for invalid in [
            NativeVisibleWorkArea {
                x: f64::NAN,
                ..valid_record()
            },
            NativeVisibleWorkArea {
                y: f64::INFINITY,
                ..valid_record()
            },
            NativeVisibleWorkArea {
                width: f64::NEG_INFINITY,
                ..valid_record()
            },
            NativeVisibleWorkArea {
                height: f64::NAN,
                ..valid_record()
            },
        ] {
            assert_eq!(validate_record(invalid), None);
        }
    }
}
