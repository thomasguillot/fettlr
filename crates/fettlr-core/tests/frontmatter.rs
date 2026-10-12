use std::collections::BTreeMap;

use fettlr_core::parse_frontmatter;

#[test]
fn parses_name_and_description() {
    let fields = parse_frontmatter(
        "---\nname: apple-design\ndescription: Apple's approach to interface design.\n---\n\n# Body\n",
    );
    assert_eq!(fields.get("name").map(String::as_str), Some("apple-design"));
    assert_eq!(
        fields.get("description").map(String::as_str),
        Some("Apple's approach to interface design.")
    );
    assert_eq!(fields.len(), 2);
}

#[test]
fn strips_surrounding_quotes() {
    let fields = parse_frontmatter(
        "---\nname: \"quoted-skill\"\ndescription: 'Single quoted, with a comma.'\n---\n",
    );
    assert_eq!(fields.get("name").map(String::as_str), Some("quoted-skill"));
    assert_eq!(
        fields.get("description").map(String::as_str),
        Some("Single quoted, with a comma.")
    );
}

#[test]
fn keeps_colons_inside_values() {
    let fields = parse_frontmatter(
        "---\nallowed-tools: Bash(agent-browser:*)\ndescription: Use when: the user asks for it.\n---\n",
    );
    assert_eq!(
        fields.get("allowed-tools").map(String::as_str),
        Some("Bash(agent-browser:*)")
    );
    assert_eq!(
        fields.get("description").map(String::as_str),
        Some("Use when: the user asks for it.")
    );
}

#[test]
fn missing_frontmatter_yields_nothing() {
    assert!(parse_frontmatter("# Just a heading\n\nname: not-frontmatter\n").is_empty());
    assert!(parse_frontmatter("").is_empty());
}

#[test]
fn unterminated_block_yields_nothing() {
    let fields =
        parse_frontmatter("---\nname: truncated\ndescription: The closing marker never arrives.\n");
    assert!(fields.is_empty());
}

#[test]
fn tolerates_leading_blank_lines_and_carriage_returns() {
    let fields = parse_frontmatter("\n\r\n---\r\nname: crlf-skill\r\n---\r\n# Body\r\n");
    assert_eq!(fields.get("name").map(String::as_str), Some("crlf-skill"));
}

#[test]
fn skips_comments_blank_lines_and_valueless_entries() {
    let fields = parse_frontmatter("---\n# a comment\n\nname: tidy\n\nnot-a-field\n---\n");
    let expected: BTreeMap<String, String> =
        BTreeMap::from([("name".to_string(), "tidy".to_string())]);
    assert_eq!(fields, expected);
}

#[test]
fn nested_entries_do_not_leak_to_the_top_level() {
    let fields =
        parse_frontmatter("---\nname: nested\nmetadata:\n  author: someone\n  version: 2\n---\n");
    assert_eq!(fields.get("name").map(String::as_str), Some("nested"));
    assert_eq!(fields.get("author"), None);
    assert_eq!(fields.get("version"), None);
}

#[test]
fn empty_value_is_an_empty_string() {
    let fields = parse_frontmatter("---\nname:\ndescription: \n---\n");
    assert_eq!(fields.get("name").map(String::as_str), Some(""));
    assert_eq!(fields.get("description").map(String::as_str), Some(""));
}

#[test]
fn last_value_wins_for_repeated_keys() {
    let fields = parse_frontmatter("---\nname: first\nname: second\n---\n");
    assert_eq!(fields.get("name").map(String::as_str), Some("second"));
}

#[test]
fn dashes_inside_the_body_do_not_reopen_or_corrupt_the_block() {
    let fields = parse_frontmatter(
        "---\nname: has-body-rule\ndescription: Body has its own dashes.\n---\n\nIntro.\n\n---\n\nnot: leaked\n",
    );
    assert_eq!(fields.len(), 2);
    assert_eq!(
        fields.get("name").map(String::as_str),
        Some("has-body-rule")
    );
    assert_eq!(fields.get("not"), None);
}

#[test]
fn a_byte_order_mark_does_not_hide_the_block() {
    let fields = parse_frontmatter("\u{feff}---\nname: bom\ndescription: still read\n---\n");
    assert_eq!(fields.get("name").map(String::as_str), Some("bom"));
    assert_eq!(
        fields.get("description").map(String::as_str),
        Some("still read")
    );
}

#[test]
fn a_block_scalar_yields_its_lines_rather_than_its_indicator() {
    let folded =
        parse_frontmatter("---\nname: folded\ndescription: >\n  first part\n  second part\n---\n");
    assert_eq!(
        folded.get("description").map(String::as_str),
        Some("first part second part")
    );

    let literal =
        parse_frontmatter("---\nname: literal\ndescription: |-\n  line one\n  line two\n---\n");
    assert_eq!(
        literal.get("description").map(String::as_str),
        Some("line one\nline two")
    );
}

#[test]
fn a_plain_scalar_wrapped_onto_indented_lines_is_folded() {
    let fields = parse_frontmatter(
        "---\nname: wrap\ndescription: Use when the user asks\n  for a release checklist\n---\n",
    );
    assert_eq!(
        fields.get("description").map(String::as_str),
        Some("Use when the user asks for a release checklist")
    );
    assert_eq!(fields.get("name").map(String::as_str), Some("wrap"));
}

#[test]
fn a_quoted_scalar_wrapped_onto_indented_lines_is_folded_and_unquoted() {
    let fields = parse_frontmatter("---\ndescription: \"Use when\n  the user\"\n---\n");
    assert_eq!(
        fields.get("description").map(String::as_str),
        Some("Use when the user")
    );
}

#[test]
fn an_empty_value_followed_by_indented_keys_gets_no_continuation() {
    let fields = parse_frontmatter("---\nmetadata:\n  author: someone\nname: n\n---\n");
    assert_eq!(fields.get("metadata").map(String::as_str), Some(""));
    assert_eq!(fields.get("name").map(String::as_str), Some("n"));
    assert_eq!(fields.len(), 2);
}

#[test]
fn an_indented_comment_line_after_a_plain_value_is_not_part_of_it() {
    let fields = parse_frontmatter("---\ndescription: foo\n  # todo\n  bar\n---\n");
    assert_eq!(
        fields.get("description").map(String::as_str),
        Some("foo bar")
    );
}
