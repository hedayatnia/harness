import XCTest
@testable import Harness

/// Math in replies reads as Unicode, like the desktop (crates/ui/src/markdown/math.rs).
/// swift-markdown read `\(w_0\)` as escaped punctuation and showed `(w_0)`.
final class MathTextTests: XCTestCase {
    private func text(_ block: TopBlock?) -> String {
        switch block?.block {
        case .paragraph(let runs)?: return runs.map(\.text).joined()
        case .codeBlock(_, let code)?: return code
        default: XCTFail("unexpected block \(String(describing: block))"); return ""
        }
    }

    func testTheReportedFormulasReadAsMath() {
        XCTAssertEqual(MathText.toUnicode("w_0"), "w₀")
        XCTAssertEqual(MathText.toUnicode(" h(x)=w_0+w_1x, "), "h(x)=w₀+w₁x,")
        XCTAssertEqual(MathText.toUnicode("x_{10}^2"), "x₁₀²")
    }

    func testCommandsFractionsRootsAndSets() {
        XCTAssertEqual(MathText.toUnicode("\\frac{1}{2}"), "1/2")
        XCTAssertEqual(MathText.toUnicode("\\frac{a+b}{n}"), "(a+b)/n")
        XCTAssertEqual(MathText.toUnicode("\\sqrt{x^2 + y^2}"), "√(x² + y²)")
        XCTAssertEqual(MathText.toUnicode("\\sqrt[3]{8}"), "³√8")
        XCTAssertEqual(MathText.toUnicode("\\sum_{i=1}^{n} x_i"), "∑ᵢ₌₁ⁿ xᵢ")
        XCTAssertEqual(MathText.toUnicode("\\alpha \\le \\beta \\cdot \\theta"), "α ≤ β · θ")
        XCTAssertEqual(MathText.toUnicode("x \\in \\mathbb{R}^n"), "x ∈ ℝⁿ")
        XCTAssertEqual(MathText.toUnicode("\\hat{y} = \\text{mean}(y)"), "ŷ = mean(y)")
        XCTAssertEqual(MathText.toUnicode("\\log n"), "log n")
        XCTAssertEqual(MathText.toUnicode("x_{max}"), "xₘₐₓ")
        XCTAssertEqual(MathText.toUnicode("x_{best}"), "x_(best)")
    }

    func testAReplyWithFormulasParsesToReadableMath() {
        let blocks = MarkdownParser.parse(
            "the parameters **\\(w_0\\) and \\(w_1\\)**, not the inputs \\(x_0\\).\n\n" +
                "For a linear regression model, \\[ h(x)=w_0+w_1x, \\] choosing one. `\\(raw\\)`"
        )
        XCTAssertEqual(blocks.count, 2)
        XCTAssertEqual(text(blocks.first), "the parameters w₀ and w₁, not the inputs x₀.")
        guard case .paragraph(let first)? = blocks.first?.block else { return XCTFail() }
        XCTAssertTrue(first.contains { $0.text.contains("w₀") && $0.style.bold })
        XCTAssertEqual(text(blocks.last), "For a linear regression model,\nh(x)=w₀+w₁x,\nchoosing one. \\(raw\\)")
        guard case .paragraph(let second)? = blocks.last?.block else { return XCTFail() }
        XCTAssertTrue(second.contains { $0.style.code && $0.text == "\\(raw\\)" }, "code stays literal")
    }

    func testDisplayMathOverSeveralLinesKeepsTheParagraphAndLineNumbers() {
        let blocks = MarkdownParser.parse("Model:\n\\[\nh(x) = w_0\n\\]\nthen fit.\n\nNext")
        XCTAssertEqual(blocks.count, 2)
        XCTAssertEqual(text(blocks.first), "Model:\nh(x) = w₀\nthen fit.")
        XCTAssertEqual(blocks.last?.startLine, 7)
    }

    func testEscapesPricesAndCodeStayLiteral() {
        XCTAssertEqual(text(MarkdownParser.parse("cite \\[1\\] and \\(optional\\)").first), "cite [1] and (optional)")
        XCTAssertEqual(text(MarkdownParser.parse("costs $5 and $10").first), "costs $5 and $10")
        XCTAssertEqual(text(MarkdownParser.parse("so \\(w_0 is").first), "so (w_0 is")
        XCTAssertEqual(text(MarkdownParser.parse("```\n\\(w_0\\)\n```").first), "\\(w_0\\)")
        XCTAssertEqual(text(MarkdownParser.parse("    \\[\n    x^2\n    \\]").first), "\\[\nx^2\n\\]")
        XCTAssertEqual(text(MarkdownParser.parse("$$a^2$$").first), "a²")
    }
}
