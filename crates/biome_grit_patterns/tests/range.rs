use biome_grit_patterns::{
    CompileError, GritQueryEffect, NodeLikeArgumentError, compile_pattern, testing::make_js_file,
};

#[test]
fn range_bounds() {
    let source = "first();\n  second(); third();\nfourth(\n  fifth()\n);\n";
    for (bounds, expected) in [
        (
            "",
            vec![
                "first()",
                "second()",
                "third()",
                "fourth(\n  fifth()\n)",
                "fifth()",
            ],
        ),
        ("start_line=2, end_line=2", vec!["second()", "third()"]),
        (
            "start_line=2, start_column=3, end_line=2, end_column=11",
            vec!["second()"],
        ),
        (
            "start_line=2, start_column=4, end_line=2, end_column=11",
            vec![],
        ),
        ("start_line=2, end_line=2, end_column=10", vec![]),
        ("end_line=1", vec!["first()"]),
        ("start_line=4", vec!["fifth()"]),
        (
            "start_line=3, end_line=5",
            vec!["fourth(\n  fifth()\n)", "fifth()"],
        ),
        ("start_line=3, end_line=4", vec!["fifth()"]),
        ("start_line=5, end_line=2", vec![]),
        ("start_line=100", vec![]),
        (
            "start_line=0, end_line=4294967295",
            vec![
                "first()",
                "second()",
                "third()",
                "fourth(\n  fifth()\n)",
                "fifth()",
            ],
        ),
    ] {
        let query = compile_pattern(&format!(
            "call_expression() as $call where {{ $call <: range({bounds}) }}"
        ))
        .unwrap();
        let result = query.execute(make_js_file(source)).unwrap();
        let matches: Vec<_> = result
            .effects
            .into_iter()
            .flat_map(|effect| match effect {
                GritQueryEffect::Match(result) => result.ranges,
                _ => panic!("unexpected effect"),
            })
            .map(|range| &source[range.start_byte as usize..range.end_byte as usize])
            .collect();
        assert_eq!(matches, expected, "range({bounds})");
    }
}

#[test]
fn invalid_range_bounds() {
    for bounds in [
        "end_line=4294967296",
        "start_line=1.5",
        "start_line=\"2\"",
        "start_line=$line",
        "start_column=1",
        "end_column=1",
        "start_line=1, end_column=1",
        "end_line=1, start_column=1",
    ] {
        assert!(
            matches!(
                compile_pattern(&format!("range({bounds})")),
                Err(CompileError::InvalidRange(_))
            ),
            "range({bounds})"
        );
    }
}

#[test]
fn invalid_range_arguments() {
    assert!(compile_pattern("range(start_line=-1)").is_err());
    assert!(compile_pattern("range(start_line=_)").is_err());
    assert!(matches!(
        compile_pattern("range(line=2)"),
        Err(CompileError::FunctionArgument(
            NodeLikeArgumentError::UnknownArgument { .. }
        ))
    ));
    assert!(matches!(
        compile_pattern("range(start_line=1, start_line=2)"),
        Err(CompileError::FunctionArgument(
            NodeLikeArgumentError::DuplicateArguments { .. }
        ))
    ));
    assert!(matches!(
        compile_pattern("range(2)"),
        Err(CompileError::FunctionArgument(
            NodeLikeArgumentError::MissingArgumentName { .. }
        ))
    ));
}

fn matched_text<'a>(query: &str, source: &'a str) -> Vec<&'a str> {
    let result = compile_pattern(query)
        .unwrap()
        .execute(make_js_file(source))
        .unwrap();
    result
        .effects
        .into_iter()
        .flat_map(|effect| match effect {
            GritQueryEffect::Match(result) => result.ranges,
            _ => panic!("unexpected effect"),
        })
        .map(|range| &source[range.start_byte as usize..range.end_byte as usize])
        .collect()
}

#[test]
fn range_columns_only_constrain_boundary_lines() {
    let source = "early(); late();\nmiddle();\nearly(); late();";
    for (bounds, expected) in [
        (
            "start_line=1, start_column=10, end_line=3, end_column=8",
            vec!["late()", "middle()", "early()"],
        ),
        (
            "start_line=1, start_column=10",
            vec!["late()", "middle()", "early()", "late()"],
        ),
        (
            "end_line=3, end_column=8",
            vec!["early()", "late()", "middle()", "early()"],
        ),
        (
            "start_line=1, start_column=10, end_line=1, end_column=8",
            vec![],
        ),
        (
            "start_line=1, start_column=10, end_line=1, end_column=10",
            vec![],
        ),
    ] {
        assert_eq!(
            matched_text(
                &format!("call_expression() as $call where {{ $call <: range({bounds}) }}"),
                source,
            ),
            expected,
            "range({bounds})",
        );
    }
}

#[test]
fn range_arguments_can_be_reordered() {
    assert_eq!(
        matched_text(
            "range(end_column=8, start_column=1, end_line=2, start_line=2) as $call where { $call <: call_expression() }",
            "first();\nfirst();\nfirst();",
        ),
        ["first()"],
    );
}

#[test]
fn range_preserves_unicode_offsets_and_crlf_lines() {
    for newline in ["\n", "\r\n"] {
        let source = format!("π();{newline}  café();{newline}π();");
        assert_eq!(
            matched_text(
                "call_expression() as $call where { $call <: range(start_line=2, start_column=3, end_line=2, end_column=9) }",
                &source,
            ),
            ["café()"],
            "newline {newline:?}",
        );
    }
}

#[test]
fn range_ignores_outer_trivia_and_blank_lines() {
    assert_eq!(
        matched_text(
            "call_expression() as $call where { $call <: range(start_line=3, start_column=3, end_line=3, end_column=10) }",
            "// header\n\n  first(); // trailing\n\nfirst();",
        ),
        ["first()"],
    );
}

#[test]
fn range_composes_with_negation_and_disjunction() {
    let source = "first();\nsecond();\nthird();";
    for predicate in [
        "! range(start_line=2, end_line=2)",
        "or { range(end_line=1), range(start_line=3) }",
    ] {
        assert_eq!(
            matched_text(
                &format!("call_expression() as $call where {{ $call <: {predicate} }}"),
                source,
            ),
            ["first()", "third()"],
            "{predicate}",
        );
    }
}

#[test]
fn range_in_pattern_definition() {
    assert_eq!(
        matched_text(
            "pattern second_line() { range(start_line=2, end_line=2) } call_expression() as $call where { $call <: second_line() }",
            "first();\nsecond();\nthird();",
        ),
        ["second()"],
    );
}

#[test]
fn range_does_not_match_values_without_positions() {
    for value in ["\"text\"", "42", "true", "[1, 2]"] {
        assert!(
            matched_text(
                &format!("call_expression() where {{ $value = {value}, $value <: range() }}"),
                "first();",
            )
            .is_empty(),
            "{value}",
        );
    }
}

#[test]
fn range_limits_rewrites_in_both_execution_modes() {
    let query = compile_pattern(
        "`first()` as $call => `second()` where { $call <: range(start_line=2, end_line=2) }",
    )
    .unwrap();
    let source = "first();\nfirst();\nfirst();";
    for result in [
        query.execute(make_js_file(source)).unwrap(),
        query.execute_optimized(make_js_file(source)).unwrap(),
    ] {
        let [GritQueryEffect::Rewrite(rewrite)] = result.effects.as_slice() else {
            panic!("expected one rewrite, got {:?}", result.effects);
        };
        assert_eq!(rewrite.rewritten.content, "first();\nsecond();\nfirst();");
    }
}

#[test]
fn range_supports_css_and_json() {
    use biome_grit_patterns::{
        CompilePatternOptions, GritTargetFile, GritTargetLanguage, compile_pattern_with_options,
    };
    use camino::Utf8Path;

    for (extension, source, pattern, expected) in [
        (
            "css",
            "a {\ncolor: red;\ncolor: blue;\n}",
            "CssDeclaration()",
            "color: red",
        ),
        (
            "json",
            "{\n\"first\": 1,\n\"second\": 2\n}",
            "JsonMember()",
            "\"first\": 1",
        ),
    ] {
        let language = GritTargetLanguage::from_extension(extension).unwrap();
        let query = compile_pattern_with_options(
            &format!("{pattern} as $node where {{ $node <: range(start_line=2, end_line=2) }}"),
            CompilePatternOptions::default().with_default_language(language.clone()),
        )
        .unwrap();
        let result = query
            .execute(GritTargetFile::parse(
                source,
                Utf8Path::new("test"),
                language,
            ))
            .unwrap();
        let matches: Vec<_> = result
            .effects
            .into_iter()
            .flat_map(|effect| match effect {
                GritQueryEffect::Match(result) => result.ranges,
                _ => panic!("unexpected effect"),
            })
            .map(|range| &source[range.start_byte as usize..range.end_byte as usize])
            .collect();
        assert_eq!(matches, [expected], "{extension}");
    }
}

#[test]
fn range_validates_every_bound() {
    for name in ["start_line", "start_column", "end_line", "end_column"] {
        let required_line = match name {
            "start_column" => "start_line=1, ",
            "end_column" => "end_line=1, ",
            _ => "",
        };
        for value in ["4294967296", "1.5", "\"2\"", "$bound", "true", "[1]"] {
            let query = format!("range({required_line}{name}={value})");
            let Err(CompileError::InvalidRange(message)) = compile_pattern(&query) else {
                panic!("expected an invalid bound error for {query}");
            };
            assert!(message.contains(name), "{query}: {message}");
        }
        let query = format!("range({name}=0, {name}=1)");
        let Err(CompileError::FunctionArgument(NodeLikeArgumentError::DuplicateArguments {
            name: duplicate,
        })) = compile_pattern(&query)
        else {
            panic!("expected duplicate argument error for {query}");
        };
        assert_eq!(duplicate, name);
    }
}

#[test]
fn range_requires_the_corresponding_line_for_each_column() {
    let bounds = [
        "start_line=1",
        "start_column=1",
        "end_line=2",
        "end_column=2",
    ];
    for mask in 0..16 {
        let args: Vec<_> = bounds
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, bound)| *bound)
            .collect();
        let query = format!("range({})", args.join(", "));
        let valid = (mask & 2 == 0 || mask & 1 != 0) && (mask & 8 == 0 || mask & 4 != 0);
        assert_eq!(compile_pattern(&query).is_ok(), valid, "{query}");
    }
}
