//! Currency, literal-region and streaming regressions for inline math.
use super::*;
use crate::markdown::parser::{Block, BlockTree, IncrementalParser, InlineRun, parse_full};

fn formulas(source: &str) -> Vec<(String, bool)> {
    spans(source, &[])
        .into_iter()
        .map(|span| (span.latex, span.display))
        .collect()
}

fn block_text(block: &Block) -> String {
    let runs_text =
        |runs: &[InlineRun]| runs.iter().map(|run| run.text.as_str()).collect::<String>();
    match block {
        Block::Paragraph { runs } | Block::Heading { runs, .. } => runs_text(runs),
        Block::CodeBlock { code, .. } => code.clone(),
        Block::BlockQuote { children } => children.iter().map(block_text).collect(),
        Block::List { items, .. } => items.iter().flatten().map(block_text).collect(),
        Block::Table { header, rows, .. } => header
            .iter()
            .chain(rows.iter().flatten())
            .map(|runs| runs_text(runs))
            .collect(),
        Block::Rule => String::new(),
    }
}

fn tree_text(tree: &BlockTree) -> String {
    tree.blocks
        .iter()
        .map(|top| block_text(&top.block))
        .collect()
}

#[test]
fn inline_dollars_accept_common_agent_math() {
    for latex in [
        "x",
        "n",
        "1",
        "0.5",
        "-2",
        "x^2",
        "x_i",
        "a+b",
        "x = y",
        "2 x",
        "α + β",
        "\\alpha",
        "\\sigma/\\sqrt m",
        "\\operatorname{Var}(\\bar X)",
        "\\frac{a+b}{n}",
        "\\text{sample mean}",
    ] {
        let source = format!("before ${latex}$ after");
        assert_eq!(formulas(&source), [(latex.to_owned(), false)], "{source}");
        let span = &spans(&source, &[])[0];
        assert_eq!(&source[span.range.clone()], format!("${latex}$"));
    }
}

#[test]
fn ordinary_prices_and_price_ranges_remain_literal() {
    for source in [
        "$5",
        "$10.99",
        "costs $5 and $10",
        "$5, $10, and $20",
        "$5–$10",
        "$5-$10",
        "$.50–$.75",
        "$.50-$.75",
        "US$5 / US$10",
        "S$5 and S$10",
        "$５–$１０",
        "$٥–$١٠",
        "$1,000 to $2,000",
        "$5.00 + $10.00",
        "$5 or $10 or $20",
        "prices: $5\nand $10",
        "$5\r\n$10",
        "$5\n\n$10",
        "\\$5 and \\$10",
        "a literal dollar: $",
        "$x$2",
        "$x$10",
    ] {
        assert!(formulas(source).is_empty(), "{source}");
    }
}

#[test]
fn prices_do_not_swallow_later_formulas() {
    for source in [
        "costs $5 and $10; use $x^2$ instead",
        "$5-$10; then $x^2$",
        "pay $5, calculate $x^2$, pay $10",
        "\\$5; $x^2$; \\$10",
        "$ unfinished\n$x^2$",
        "$x +\n$x^2$",
        "$x\r$x^2$",
    ] {
        assert_eq!(formulas(source), [("x^2".into(), false)], "{source}");
    }
}

#[test]
fn dollar_edges_require_non_whitespace_and_matching_delimiters() {
    for source in [
        "$ x$",
        "$x $",
        "$\tx$",
        "$x\t$",
        "$\u{a0}x$",
        "$x\u{a0}$",
        "$x\ny$",
        "$x\ry$",
        "$x\r\ny$",
        "$x",
        "$",
        "$$",
        "$$$x$$$",
        "$x$$",
        "$x$2",
        "\\$x$",
        "$x\\$",
    ] {
        assert!(formulas(source).is_empty(), "{source:?}");
    }
    assert_eq!(formulas("$n$th"), [("n".into(), false)]);
    assert_eq!(
        formulas("($x$), **$y$**."),
        [("x".into(), false), ("y".into(), false)]
    );
}

#[test]
fn escaped_dollars_and_even_backslashes_have_distinct_meanings() {
    assert_eq!(formulas(r"$x+\$1$"), [(r"x+\$1".into(), false)]);
    assert_eq!(formulas(r"\\$x$"), [("x".into(), false)]);
    assert!(formulas(r"\\\$x$").is_empty());
    assert_eq!(formulas(r"$x\\$"), [(r"x\\".into(), false)]);
    assert_eq!(formulas(r"$$x+\$$y$$"), [(r"x+\$$y".into(), true)]);
    assert_eq!(formulas(r"\$$x$"), [("x".into(), false)]);
    assert_eq!(formulas(r"\$$$x^2$$"), [("x^2".into(), true)]);
    assert_eq!(formulas(r"$\$$"), [(r"\$".into(), false)]);
}

#[test]
fn all_four_delimiters_can_coexist() {
    assert_eq!(
        formulas(r"$a$ and \(b\), $$c^2$$ and \[d^2\]."),
        [
            ("a".into(), false),
            ("b".into(), false),
            ("c^2".into(), true),
            ("d^2".into(), true)
        ]
    );
    assert_eq!(
        formulas("$$x^2$$ $y$"),
        [("x^2".into(), true), ("y".into(), false)]
    );
    assert_eq!(formulas("$unclosed $$y^2$$"), [("y^2".into(), true)]);
    assert!(formulas("$$x^2$$$").is_empty());
    assert_eq!(formulas("$$x^2$$$ tail $y$"), [("y".into(), false)]);
}

#[test]
fn formulas_render_in_bold_lists_quotes_headings_and_tables() {
    let source = "# $x^2$\n\n**$x_i$** and $\\alpha$.\n\n- $\\sqrt{x}$\n\n> $\\beta$\n\n| Formula |\n|---|\n| $\\sigma/\\sqrt m$ |\n";
    let tree = parse_full(source);
    let text = tree_text(&tree);
    for expected in ["x²", "xᵢ", "α", "√x", "β", "σ/√m"] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    assert!(!text.contains('$'), "{text}");
    let Block::Paragraph { runs } = &tree.blocks[1].block else {
        panic!("paragraph missing")
    };
    assert!(runs.iter().any(|run| run.text == "xᵢ" && run.style.bold));
}

#[test]
fn code_links_images_and_html_blocks_are_not_rewritten() {
    for source in [
        "`$x^2$`",
        "``$x^2$``",
        "```tex\n$x^2$\n```",
        "~~~\n$x^2$\n~~~",
        "```tex\n$x^2$",
        "    $x^2$\n",
        "[label $x^2$](https://example.com)",
        "![alt $x^2$](image.png)",
        "<div>\n$x^2$\n</div>\n",
        "<span data-formula='$x^2$'>literal</span>",
    ] {
        let text = tree_text(&parse_full(source));
        assert!(text.contains("$x^2$"), "{source:?} became {text:?}");
    }
    assert_eq!(
        tree_text(&parse_full("`$x^2$` then $y^2$")),
        "$x^2$ then y²"
    );
}

#[test]
fn a_formula_must_not_cross_a_literal_region() {
    for (source, protected) in [
        ("$x `code` + y$", 3..9),
        ("$x `code$` y$", 3..10),
        (r"\(x `code` + y\)", 4..10),
        ("$$x `code` + y$$", 4..10),
    ] {
        assert!(spans(source, &[protected]).is_empty(), "{source}");
    }
    assert_eq!(tree_text(&parse_full("$x `code$` y$")), "$x code$ y$");
}

#[test]
fn directive_payload_dollars_do_not_consume_adjacent_math() {
    let source = r#":codex-file-citation{path="/tmp/$draft.pdf"}$w_0$"#;
    assert_eq!(tree_text(&parse_full(source)), "$draft.pdfw₀");
    let source = "\u{e200}visualize\u{e202}{\"path\":\"/tmp/$draft.html\"}\u{e201}$w_0$";
    assert_eq!(tree_text(&parse_full(source)), "Preview · $draft.htmlw₀");
}

#[test]
fn rewriting_preserves_original_ranges_and_task_marker_offsets() {
    let source = "α $x^2$ first.\n\n- [ ] $y_i$ last.\n";
    let tree = parse_full(source);
    let second = &tree.blocks[1];
    assert!(source[second.range.clone()].starts_with("- [ ]"));
    let Block::List { items, .. } = &second.block else {
        panic!("list missing")
    };
    let Block::Paragraph { runs } = &items[0][0] else {
        panic!("list paragraph missing")
    };
    let marker = runs.iter().find_map(|run| run.style.task.as_ref()).unwrap();
    assert_eq!(&source[marker.range.clone()], "[ ]");
    assert!(tree_text(&tree).contains("yᵢ"));
}

#[test]
fn oversized_or_unclosed_formulas_do_not_block_later_math() {
    let oversized = "x".repeat(MAX_MATH_BYTES + 1);
    let source = format!("${oversized}$\n$y$");
    assert_eq!(formulas(&source), [("y".into(), false)]);
    let at_limit = format!("${}$", "x".repeat(MAX_MATH_BYTES));
    assert_eq!(spans(&at_limit, &[]).len(), 1);
    let multibyte_limit = format!("${}$", "é".repeat(MAX_MATH_BYTES / 2));
    assert_eq!(spans(&multibyte_limit, &[]).len(), 1);
    assert!(spans(&multibyte_limit.replace("$", "a$"), &[]).is_empty());
    let source = format!("$x{}\n$y$", " ".repeat(32 * 1024));
    assert_eq!(formulas(&source), [("y".into(), false)]);
}

#[test]
fn excessive_group_and_command_recursion_falls_back_without_losing_source() {
    for latex in [
        format!("{}x{}", "{".repeat(2048), "}".repeat(2048)),
        format!("{}x", "\\sqrt".repeat(1024)),
        format!("{}x{}", "\\text{".repeat(512), "}".repeat(512)),
    ] {
        assert!(latex.len() < MAX_MATH_BYTES);
        assert_eq!(to_unicode(&latex), latex);
        assert_eq!(tree_text(&parse_full(&format!("${latex}$"))), latex);
    }
    assert_eq!(
        to_unicode("\\frac{\\sqrt{x^2}}{\\sqrt{y_i}}"),
        "(√x²)/(√yᵢ)"
    );
}

#[test]
fn display_formulas_do_not_cross_any_commonmark_paragraph_break() {
    for gap in ["\n\n", "\r\r", "\r\n\r\n", "\n \t\n", "\r \r"] {
        let source = format!("\\[x^2{gap}y^2\\]");
        assert!(formulas(&source).is_empty(), "{source:?}");
    }
    for newline in ["\n", "\r", "\r\n"] {
        let source = format!("\\[x^2{newline}+ y^2\\]");
        assert_eq!(formulas(&source), [(format!("x^2{newline}+ y^2"), true)]);
    }
    let source = concat!(r"**\)\${```x_i\[\(5", "\r\r", r"**\\\]");
    let mut parser = IncrementalParser::new();
    parser.set_text(source);
    assert!(!tree_text(parser.tree()).contains("harness-content-reference"));
    assert!(!tree_text(&parser.display_tree()).contains("harness-content-reference"));
}

#[test]
fn generated_delimiter_sequences_keep_ranges_and_incremental_parsing_sound() {
    let tokens = [
        "$", "\\$", "$$", "\\(", "\\)", "\\[", "\\]", "α", "x_i", "5", " ", "\n", "`", "```", "**",
        "|", "\r", "{", "}", "[x]", "\\\\",
    ];
    let mut state = 0x4d415448u64;
    for case in 0..4096 {
        let mut source = String::new();
        for _ in 0..16 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            source.push_str(tokens[state as usize % tokens.len()]);
        }
        let found = spans(&source, &[]);
        let mut previous_end = 0;
        for span in found {
            assert!(span.range.start >= previous_end);
            assert!(span.range.end <= source.len());
            assert!(source.is_char_boundary(span.range.start));
            assert!(source.is_char_boundary(span.range.end));
            assert!(span.latex.len() <= MAX_MATH_BYTES);
            previous_end = span.range.end;
        }
        let mut parser = IncrementalParser::new();
        for (at, c) in source.char_indices() {
            let end = at + c.len_utf8();
            parser.set_text(&source[..end]);
            assert_eq!(
                parser.tree(),
                &parse_full(&source[..end]),
                "case {case}, byte {end}: {source:?}"
            );
            let displayed = tree_text(&parser.display_tree());
            assert!(
                !displayed.contains("harness-content-reference"),
                "case {case}, byte {end}: {:?} displayed as {displayed:?}",
                &source[..end]
            );
        }
    }
}

#[test]
fn parenthesized_formulas_after_prices_keep_their_own_opener() {
    for source in [
        "Pay $5 ($x^2$).",
        "Pay $.50 ($x^2$).",
        "Pay $1,000 [ $x^2$ ].",
        "Pay $５ (($x^2$)).",
        "Pay $٥ {$x^2$}.",
    ] {
        assert_eq!(formulas(source), [("x^2".into(), false)], "{source}");
        let expected = source.replace("$x^2$", "x²");
        assert_eq!(tree_text(&parse_full(source)), expected, "{source}");
        let mut parser = IncrementalParser::new();
        for (at, c) in source.char_indices() {
            let end = at + c.len_utf8();
            parser.set_text(&source[..end]);
            assert_eq!(
                parser.tree(),
                &parse_full(&source[..end]),
                "{end}: {source}"
            );
        }
        assert_eq!(tree_text(parser.tree()), expected);
    }
    for latex in ["5", "5 (x)", "5 [x]", "5 {x}", "5 + x", "0.5", "2 x"] {
        assert_eq!(formulas(&format!("${latex}$")), [(latex.into(), false)]);
    }
}

#[test]
fn reference_definition_destinations_and_titles_remain_literal() {
    use crate::markdown::content_reference::rewrite;
    for (definition, destination) in [
        (
            "[ref]: https://example.test/$x$",
            "https://example.test/$x$",
        ),
        (
            "[ref]: https://example.test \"Formula $x$\"",
            "https://example.test",
        ),
        (
            "[ref]: <https://example.test/$x$>\n  'Title $y$'",
            "https://example.test/$x$",
        ),
        (
            "[ref]:\n  https://example.test/$x$",
            "https://example.test/$x$",
        ),
    ] {
        let literal = format!("Read [docs][ref].\n\n{definition}\n");
        assert!(rewrite(&literal).is_none(), "{literal}");
        let source = format!("{literal}\nNow $z^2$.\n");
        let rewritten = rewrite(&source).unwrap();
        assert!(rewritten.text.contains(definition), "{source}");
        let tree = parse_full(&source);
        let Block::Paragraph { runs } = &tree.blocks[0].block else {
            panic!("paragraph missing")
        };
        let link = runs.iter().find(|run| run.text == "docs").unwrap();
        assert_eq!(link.style.link.as_deref(), Some(destination), "{source}");
        assert!(tree_text(&tree).ends_with("Now z²."));
        let mut parser = IncrementalParser::new();
        for (at, c) in source.char_indices() {
            let end = at + c.len_utf8();
            parser.set_text(&source[..end]);
            assert_eq!(
                parser.tree(),
                &parse_full(&source[..end]),
                "{end}: {source}"
            );
        }
        assert_eq!(parser.tree(), &tree);
    }
}

#[test]
fn escaped_dollars_can_touch_math_closers() {
    assert_eq!(formulas(r"$x\$$"), [(r"x\$".into(), false)]);
    assert_eq!(formulas(r"$$x\$$$"), [(r"x\$".into(), true)]);
    assert_eq!(tree_text(&parse_full(r"$x\$$")), "x$");
    assert_eq!(tree_text(&parse_full(r"$$x\$$$")), "x$");
}

#[test]
fn every_streaming_prefix_matches_a_full_parse_including_unicode() {
    for source in [
        "Price $5 or $10; α uses $x_i$ and $\\sigma/\\sqrt m$.\n\nNext $y$.",
        "**$x_i$** and $$\\frac{a}{b}$$.\n\n| Expr |\n|---|\n| $α^2$ |\n",
        "`$x$` then $y$.\n\n```tex\n$x^2$\n```\n\n$z$",
        "Escaped \\$5 and $x+\\$1$.\n\n- [ ] $y_i$\n",
        "$unfinished\n\n\\(x_i\\) and $z$",
        r#":codex-file-citation{path="/tmp/$draft.pdf"}$w_0$"#,
        r"\$$x$ and \$$$y^2$$",
        "$x$2 and $y$$",
        "first\nsecond\r****\n$x^2$",
        "$x_i$\n\n[ref]\n\nsecond\r[ref]: https://example.com",
    ] {
        let mut parser = IncrementalParser::new();
        for (at, c) in source.char_indices() {
            let end = at + c.len_utf8();
            parser.set_text(&source[..end]);
            assert_eq!(
                parser.tree(),
                &parse_full(&source[..end]),
                "prefix {end}: {source}"
            );
        }
        assert_eq!(&parser.display_tree(), &parse_full(source));
    }
}
