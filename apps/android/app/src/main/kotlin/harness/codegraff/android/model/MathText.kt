package harness.codegraff.android.model

/**
 * Math in replies (crates/ui/src/markdown/math.rs). Models write LaTeX between `\(`…`\)`,
 * `\[`…`\]` or `$$`…`$$`; these spans render as readable Unicode (`w₀`, `h(x) = w₀ + w₁x`,
 * `≤`, `∑`, `a/b`) instead of raw LaTeX. A single `$` is left alone, since prices would
 * read as math.
 */
object MathText {
    private const val MAX_MATH_CHARS = 8 * 1024

    /** A span found at [start], [end] exclusive, delimiters included. */
    data class Span(val start: Int, val end: Int, val latex: String, val display: Boolean)

    /** The complete math span opening at [at] in [s], if one does. */
    fun spanAt(s: String, at: Int): Span? {
        if (at + 1 >= s.length) return null
        val (close, display) = when {
            s.startsWith("\\(", at) -> "\\)" to false
            s.startsWith("\\[", at) -> "\\]" to true
            s.startsWith("$$", at) -> "$$" to true
            else -> return null
        }
        if (backslashesBefore(s, at) % 2 == 1) return null
        val bodyStart = at + 2
        val closeAt = findClose(s, bodyStart, close) ?: return null
        val body = s.substring(bodyStart, closeAt)
        if (body.length > MAX_MATH_CHARS || body.contains("\n\n") || !looksLikeMath(body)) return null
        return Span(at, closeAt + close.length, body.trim(), display)
    }

    private fun backslashesBefore(s: String, at: Int): Int {
        var n = 0
        var i = at - 1
        while (i >= 0 && s[i] == '\\') { n++; i-- }
        return n
    }

    private fun findClose(s: String, from: Int, close: String): Int? {
        var at = from
        while (true) {
            val found = s.indexOf(close, at)
            if (found < 0 || found - from > MAX_MATH_CHARS) return null
            val escaped = close.startsWith("\\") && backslashesBefore(s, found) % 2 == 1
            if (!escaped) return found
            at = found + close.length
        }
    }

    /** `\[1\]` and `\(optional\)` are escaped punctuation, not math. */
    private fun looksLikeMath(body: String): Boolean {
        val t = body.trim()
        if (t.isEmpty()) return false
        if (t.any { it in "\\^_=+<>{}|" }) return true
        return t.length <= 4 && t.any { it.isLetter() }
    }

    /** LaTeX as readable Unicode. Unknown commands keep their name. */
    fun toUnicode(latex: String): String {
        val out = StringBuilder()
        Converter(latex).convertInto(out, null)
        return tidy(out.toString())
    }

    private class Converter(val s: String) {
        var at = 0
        fun peek(): Char? = s.getOrNull(at)

        fun convertInto(out: StringBuilder, stop: Char?) {
            while (true) {
                val c = peek() ?: return
                if (c == stop) { at++; return }
                at++
                when (c) {
                    '\\' -> command(out)
                    '{' -> convertInto(out, '}')
                    '}' -> {}
                    '^' -> out.append(script(argument(), sup = true))
                    '_' -> out.append(script(argument(), sup = false))
                    '~', '&' -> out.append(' ')
                    '\'' -> out.append('′')
                    else -> out.append(c)
                }
            }
        }

        fun argument(): String {
            while (peek() == ' ') at++
            val out = StringBuilder()
            when (val c = peek()) {
                '{' -> { at++; convertInto(out, '}') }
                '\\' -> { at++; command(out) }
                null -> {}
                else -> { at++; out.append(c) }
            }
            return out.toString()
        }

        fun optional(): String? {
            if (peek() != '[') return null
            at++
            val out = StringBuilder()
            while (true) {
                val c = peek() ?: break
                at++
                if (c == ']') break
                out.append(c)
            }
            return out.toString()
        }

        fun command(out: StringBuilder) {
            val first = peek() ?: return
            if (!first.isAsciiLetter()) {
                at++
                when (first) {
                    ',', ';', ':', ' ' -> out.append(' ')
                    '!' -> {}
                    '\\' -> out.append('\n')
                    else -> out.append(first)
                }
                return
            }
            val name = StringBuilder()
            while (peek()?.isAsciiLetter() == true) { name.append(peek()); at++ }
            when (val n = name.toString()) {
                "frac", "dfrac", "tfrac" -> {
                    val num = argument(); val den = argument()
                    out.append(group(num)).append('/').append(group(den))
                }
                "sqrt" -> {
                    val index = optional(); val arg = argument()
                    if (index != null) out.append(script(index, sup = true))
                    out.append('√').append(group(arg))
                }
                "text", "mathrm", "mathit", "mathbf", "mathsf", "mathtt", "operatorname",
                "textbf", "textit", "boldsymbol", "bm", "mbox" -> out.append(argument())
                "mathbb" -> out.append(argument().map(::doubleStruck).joinToString(""))
                "hat", "widehat" -> accent(out, argument(), '̂')
                "bar", "overline" -> accent(out, argument(), '̄')
                "tilde", "widetilde" -> accent(out, argument(), '̃')
                "vec" -> accent(out, argument(), '⃗')
                "dot" -> accent(out, argument(), '̇')
                "ddot" -> accent(out, argument(), '̈')
                "left", "right", "big", "Big", "bigg", "Bigg", "displaystyle", "textstyle", "limits", "nolimits" -> {}
                "quad" -> out.append("  ")
                "qquad" -> out.append("    ")
                "begin", "end" -> argument()
                else -> out.append(SYMBOLS[n] ?: n)
            }
        }

        private fun Char.isAsciiLetter() = this in 'a'..'z' || this in 'A'..'Z'
    }

    private fun group(s: String): String {
        val t = s.trim()
        val simple = t.length <= 1 || t.all { it.isLetterOrDigit() || it == '.' || it == '′' } ||
            (t.startsWith("(") && t.endsWith(")"))
        return if (simple) t else "($t)"
    }

    private fun accent(out: StringBuilder, arg: String, mark: Char) {
        out.append(arg)
        if (arg.isNotEmpty()) out.append(mark)
    }

    private fun script(arg: String, sup: Boolean): String {
        val t = arg.trim()
        val table = if (sup) SUPERSCRIPT else SUBSCRIPT
        val mapped = t.map { table[it] }
        if (t.isNotEmpty() && mapped.all { it != null }) return mapped.joinToString("")
        val mark = if (sup) '^' else '_'
        return if (t.length == 1) "$mark$t" else "$mark($t)"
    }

    private fun doubleStruck(c: Char): String = when (c) {
        'R' -> "ℝ"; 'N' -> "ℕ"; 'Z' -> "ℤ"; 'Q' -> "ℚ"; 'C' -> "ℂ"; 'P' -> "ℙ"; 'E' -> "𝔼"
        else -> c.toString()
    }

    private fun tidy(s: String): String =
        s.lines().joinToString("\n") { line -> line.split(Regex("\\s+")).filter { it.isNotEmpty() }.joinToString(" ") }.trim()

    private fun pairs(keys: String, values: String): Map<Char, Char> = keys.zip(values).toMap()

    private val SUPERSCRIPT: Map<Char, Char> =
        pairs("0123456789+-=()abcdefghijklmnoprstuvwxyzT′* ", "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾ᵃᵇᶜᵈᵉᶠᵍʰⁱʲᵏˡᵐⁿᵒᵖʳˢᵗᵘᵛʷˣʸᶻᵀ′* ") + mapOf('−' to '⁻')

    private val SUBSCRIPT: Map<Char, Char> =
        pairs("0123456789+-=()aehijklmnoprstuvxβγρφχ, ", "₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎ₐₑₕᵢⱼₖₗₘₙₒₚᵣₛₜᵤᵥₓᵦᵧᵨᵩᵪ, ") + mapOf('−' to '₋')

    private val SYMBOLS: Map<String, String> = mapOf(
        "alpha" to "α", "beta" to "β", "gamma" to "γ", "delta" to "δ", "epsilon" to "ε", "varepsilon" to "ε",
        "zeta" to "ζ", "eta" to "η", "theta" to "θ", "vartheta" to "ϑ", "iota" to "ι", "kappa" to "κ",
        "lambda" to "λ", "mu" to "μ", "nu" to "ν", "xi" to "ξ", "pi" to "π", "varpi" to "ϖ", "rho" to "ρ",
        "varrho" to "ϱ", "sigma" to "σ", "varsigma" to "ς", "tau" to "τ", "upsilon" to "υ", "phi" to "φ",
        "varphi" to "φ", "chi" to "χ", "psi" to "ψ", "omega" to "ω",
        "Gamma" to "Γ", "Delta" to "Δ", "Theta" to "Θ", "Lambda" to "Λ", "Xi" to "Ξ", "Pi" to "Π",
        "Sigma" to "Σ", "Upsilon" to "Υ", "Phi" to "Φ", "Psi" to "Ψ", "Omega" to "Ω",
        "cdot" to "·", "times" to "×", "div" to "÷", "pm" to "±", "mp" to "∓", "ast" to "∗", "star" to "⋆",
        "circ" to "∘", "bullet" to "•", "le" to "≤", "leq" to "≤", "ge" to "≥", "geq" to "≥", "ne" to "≠",
        "neq" to "≠", "approx" to "≈", "equiv" to "≡", "sim" to "∼", "simeq" to "≃", "cong" to "≅",
        "propto" to "∝", "ll" to "≪", "gg" to "≫", "infty" to "∞", "partial" to "∂", "nabla" to "∇",
        "sum" to "∑", "prod" to "∏", "int" to "∫", "iint" to "∬", "oint" to "∮", "in" to "∈", "notin" to "∉",
        "ni" to "∋", "subset" to "⊂", "subseteq" to "⊆", "supset" to "⊃", "supseteq" to "⊇", "cup" to "∪",
        "cap" to "∩", "emptyset" to "∅", "varnothing" to "∅", "setminus" to "∖", "forall" to "∀",
        "exists" to "∃", "neg" to "¬", "lnot" to "¬", "land" to "∧", "wedge" to "∧", "lor" to "∨", "vee" to "∨",
        "to" to "→", "rightarrow" to "→", "leftarrow" to "←", "gets" to "←", "leftrightarrow" to "↔",
        "Rightarrow" to "⇒", "implies" to "⇒", "Leftarrow" to "⇐", "Leftrightarrow" to "⇔", "iff" to "⇔",
        "mapsto" to "↦", "uparrow" to "↑", "downarrow" to "↓", "ldots" to "…", "dots" to "…", "dotsc" to "…",
        "dotsb" to "…", "cdots" to "⋯", "vdots" to "⋮", "ddots" to "⋱", "prime" to "′", "degree" to "°",
        "angle" to "∠", "perp" to "⊥", "parallel" to "∥", "mid" to "|", "vert" to "|", "lvert" to "|",
        "rvert" to "|", "Vert" to "‖", "lVert" to "‖", "rVert" to "‖", "langle" to "⟨", "rangle" to "⟩",
        "lfloor" to "⌊", "rfloor" to "⌋", "lceil" to "⌈", "rceil" to "⌉", "lbrace" to "{", "rbrace" to "}",
        "hbar" to "ℏ", "ell" to "ℓ", "Re" to "ℜ", "Im" to "ℑ", "aleph" to "ℵ", "therefore" to "∴",
        "because" to "∵", "top" to "⊤", "bot" to "⊥", "oplus" to "⊕", "otimes" to "⊗", "dagger" to "†",
    )
}
