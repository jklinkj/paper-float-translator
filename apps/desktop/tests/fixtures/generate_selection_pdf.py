#!/usr/bin/env python3
"""Generate the deterministic Preview selection baseline PDF."""

from pathlib import Path

from reportlab.lib.colors import HexColor
from reportlab.lib.pagesizes import letter
from reportlab.pdfgen import canvas


OUTPUT = Path(__file__).with_name("selection-baseline.pdf")


def draw_wrapped_text(pdf: canvas.Canvas, x: float, y: float, lines: list[str], leading: float = 18) -> float:
    text = pdf.beginText(x, y)
    text.setFont("Helvetica", 11)
    text.setLeading(leading)
    for line in lines:
        text.textLine(line)
    pdf.drawText(text)
    return y - leading * len(lines)


def main() -> None:
    pdf = canvas.Canvas(
        str(OUTPUT),
        pagesize=letter,
        pageCompression=1,
        invariant=1,
    )
    width, height = letter
    pdf.setTitle("Paper Float Selection Baseline")
    pdf.setAuthor("Paper Float Translator")
    pdf.setSubject("Deterministic selectable-text runtime fixture")

    pdf.setFillColor(HexColor("#202326"))
    pdf.setFont("Helvetica-Bold", 20)
    pdf.drawString(54, height - 62, "Selectable Text Reliability Fixture")
    pdf.setFillColor(HexColor("#66707a"))
    pdf.setFont("Courier", 8)
    pdf.drawString(54, height - 79, "paper-float-selection-baseline-v1")

    y = height - 116
    pdf.setFillColor(HexColor("#202326"))
    pdf.setFont("Helvetica-Bold", 12)
    pdf.drawString(54, y, "Gesture baseline")
    y = draw_wrapped_text(
        pdf,
        54,
        y - 22,
        [
            "Reliable selection should support mouse dragging, double-clicked words,",
            "triple-clicked paragraphs, Shift-extended ranges, and keyboard selections.",
        ],
    )

    y -= 22
    pdf.setFont("Helvetica-Bold", 12)
    pdf.drawString(54, y, "First repeated location")
    pdf.setStrokeColor(HexColor("#1f78ad"))
    pdf.setLineWidth(2)
    pdf.line(54, y - 29, 54, y - 8)
    y = draw_wrapped_text(pdf, 64, y - 22, ["The same selection appears here."])

    y -= 25
    pdf.setFont("Helvetica-Bold", 12)
    pdf.drawString(54, y, "Long selection")
    y = draw_wrapped_text(
        pdf,
        54,
        y - 22,
        [
            "A newer selection must invalidate every delayed read and translation created",
            "for an older selection. Closing the popup must also prevent stale work from",
            "making the window visible again. This paragraph exercises wrapping, popup",
            "resizing, and scrolling without relying on network-generated content.",
        ],
    )

    y -= 34
    pdf.setFont("Helvetica-Bold", 12)
    pdf.drawString(54, y, "Second repeated location")
    pdf.setStrokeColor(HexColor("#1f78ad"))
    pdf.line(54, y - 29, 54, y - 8)
    y = draw_wrapped_text(pdf, 64, y - 22, ["The same selection appears here."])

    y -= 25
    pdf.setFont("Helvetica-Bold", 12)
    pdf.drawString(54, y, "PDF extraction boundary")
    y = draw_wrapped_text(
        pdf,
        54,
        y - 22,
        [
            "The pro-",
            "posed method keeps state-",
            "of-the-art terminology intact.",
        ],
    )

    pdf.setFillColor(HexColor("#66707a"))
    pdf.setFont("Helvetica", 8)
    pdf.drawRightString(width - 54, 34, "Preview baseline - page 1 of 1")
    pdf.showPage()
    pdf.save()


if __name__ == "__main__":
    main()
