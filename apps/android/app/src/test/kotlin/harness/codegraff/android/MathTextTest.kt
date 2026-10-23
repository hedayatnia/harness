package harness.codegraff.android

import harness.codegraff.android.model.MathText
import harness.codegraff.android.model.MdBlock
import harness.codegraff.android.model.parseMarkdown
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Math in replies reads as Unicode, like the desktop (crates/ui/src/markdown/math.rs). */
class MathTextTest {
    @Test
    fun theReportedFormulasReadAsMath() {
        assertEquals("w₀", MathText.toUnicode("w_0"))
        assertEquals("h(x)=w₀+w₁x,", MathText.toUnicode(" h(x)=w_0+w_1x, "))
        assertEquals("x₁₀²", MathText.toUnicode("x_{10}^2"))
    }

    @Test
    fun commandsFractionsRootsAndSets() {
        assertEquals("1/2", MathText.toUnicode("\\frac{1}{2}"))
        assertEquals("(a+b)/n", MathText.toUnicode("\\frac{a+b}{n}"))
        assertEquals("√(x² + y²)", MathText.toUnicode("\\sqrt{x^2 + y^2}"))
        assertEquals("³√8", MathText.toUnicode("\\sqrt[3]{8}"))
        assertEquals("∑ᵢ₌₁ⁿ xᵢ", MathText.toUnicode("\\sum_{i=1}^{n} x_i"))
        assertEquals("α ≤ β · θ", MathText.toUnicode("\\alpha \\le \\beta \\cdot \\theta"))
        assertEquals("x ∈ ℝⁿ", MathText.toUnicode("x \\in \\mathbb{R}^n"))
        assertEquals("ŷ = mean(y)", MathText.toUnicode("\\hat{y} = \\text{mean}(y)"))
        assertEquals("log n", MathText.toUnicode("\\log n"))
        assertEquals("xₘₐₓ", MathText.toUnicode("x_{max}"))
        assertEquals("x_(best)", MathText.toUnicode("x_{best}"))
    }

    @Test
    fun spansSkipEscapedPunctuationPricesAndUnclosedMath() {
        val s = "weights \\(w_0\\) and \\[ h(x)=w_0+w_1x \\] and \$\$a^2\$\$"
        val inline = MathText.spanAt(s, s.indexOf("\\("))!!
        assertEquals("w_0", inline.latex)
        assertTrue(!inline.display)
        assertEquals("h(x)=w_0+w_1x", MathText.spanAt(s, s.indexOf("\\["))!!.latex)
        assertEquals("a^2", MathText.spanAt(s, s.indexOf("$$"))!!.latex)
        assertNull(MathText.spanAt("cite \\[1\\]", 5))
        assertNull(MathText.spanAt("\\(optional\\)", 0))
        assertNull(MathText.spanAt("costs $5 and $10", 6))
        assertNull(MathText.spanAt("so \\(w_0 is", 3))
        assertNull(MathText.spanAt("a \\\\(x\\)", 3))
    }

    @Test
    fun aReplyWithFormulasParsesToReadableMath() {
        val blocks = parseMarkdown(
            "the parameters **\\(w_0\\) and \\(w_1\\)**, not the inputs \\(x_0\\).\n\n" +
                "For a linear regression model, \\[ h(x)=w_0+w_1x, \\] choosing one. `\\(raw\\)`",
        )
        val first = (blocks[0] as MdBlock.Paragraph).spans
        assertEquals("the parameters w₀ and w₁, not the inputs x₀.", first.joinToString("") { it.text })
        assertTrue(first.any { it.text.contains("w₀") && it.bold })
        val second = (blocks[1] as MdBlock.Paragraph).spans
        assertEquals("For a linear regression model,\nh(x)=w₀+w₁x,\nchoosing one. \\(raw\\)", second.joinToString("") { it.text })
        assertTrue("code stays literal", second.any { it.code && it.text == "\\(raw\\)" })
    }
}
