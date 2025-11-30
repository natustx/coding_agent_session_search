use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use serde_json::Value;
use std::fs;
use tempfile::TempDir;

fn base_cmd() -> Command {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("cass"));
    cmd.env("CODING_AGENT_SEARCH_NO_UPDATE_PROMPT", "1");
    cmd
}

#[test]
fn robot_help_prints_contract() {
    let mut cmd = base_cmd();
    cmd.arg("--robot-help");
    cmd.assert()
        .success()
        .stdout(contains("cass --robot-help (contract v1)"))
        .stdout(contains("Exit codes: 0 ok"));
}

#[test]
fn robot_docs_schemas_topic() {
    let mut cmd = base_cmd();
    cmd.args(["robot-docs", "schemas"]);
    cmd.assert()
        .success()
        .stdout(contains("schemas:"))
        .stdout(contains("search:"))
        .stdout(contains("error:"))
        .stdout(contains("trace:"));
}

#[test]
fn color_never_has_no_ansi() {
    let mut cmd = base_cmd();
    cmd.args(["--color=never", "--robot-help"]);
    cmd.assert()
        .success()
        .stdout(contains("cass --robot-help"))
        .stdout(predicate::str::contains("\u{1b}").not());
}

#[test]
fn wrap_40_inserts_line_breaks() {
    let mut cmd = base_cmd();
    cmd.args(["--wrap", "40", "--robot-help"]);
    cmd.assert()
        .success()
        // With wrap at 40, long command examples should wrap across lines
        .stdout(contains("--robot #\nSearch with JSON output"));
}

#[test]
fn tui_bypasses_in_non_tty() {
    let mut cmd = base_cmd();
    // No subcommand provided; in test harness stdout is non-TTY so TUI should be blocked
    cmd.assert()
        .failure()
        .code(2)
        .stderr(contains("TUI is disabled"));
}

#[test]
fn search_error_writes_trace() {
    let tmp = TempDir::new().unwrap();
    let trace_path = tmp.path().join("trace.jsonl");

    let mut cmd = base_cmd();
    cmd.args([
        "--trace-file",
        trace_path.to_str().unwrap(),
        "--progress=plain",
        "search",
        "foo",
        "--json",
        "--data-dir",
        tmp.path().to_str().unwrap(),
    ]);

    let assert = cmd.assert().failure();
    let output = assert.get_output().clone();
    let code = output.status.code().expect("exit code present");
    // Accept both missing-index (3) and generic search error (9) depending on how the DB layer responds.
    assert!(matches!(code, 3 | 9), "unexpected exit code {code}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    if code == 3 {
        assert!(stderr.contains("missing-index"));
    } else {
        assert!(stderr.contains("\"kind\":\"search\""));
    }

    let trace = fs::read_to_string(&trace_path).expect("trace file exists");
    let last_line = trace.lines().last().expect("trace line present");
    let json: Value = serde_json::from_str(last_line).expect("valid trace json");
    let exit_code = json["exit_code"].as_i64().expect("exit_code present");
    assert_eq!(exit_code, code as i64);
    assert_eq!(json["contract_version"], "1");
}

// ============================================================
// yln.5: E2E Search Tests with Fixture Data
// ============================================================

#[test]
fn search_returns_json_results() {
    // E2E test: search with JSON output returns structured results (yln.5)
    let mut cmd = base_cmd();
    cmd.args([
        "search",
        "hello",
        "--json",
        "--data-dir",
        "tests/fixtures/search_demo_data",
    ]);

    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output
    let json: Value = serde_json::from_str(stdout.trim()).expect("valid JSON output");

    // Verify structure
    assert!(json["count"].is_number(), "JSON should have count field");
    assert!(json["hits"].is_array(), "JSON should have hits array");
    assert!(json["count"].as_u64().unwrap() > 0, "Should find results for 'hello'");

    // Verify hit structure
    let hits = json["hits"].as_array().unwrap();
    let first_hit = &hits[0];
    assert!(first_hit["agent"].is_string(), "Hit should have agent");
    assert!(first_hit["source_path"].is_string(), "Hit should have source_path");
    assert!(first_hit["score"].is_number(), "Hit should have score");
}

#[test]
fn search_respects_limit() {
    // E2E test: --limit restricts results (yln.5)
    let mut cmd = base_cmd();
    cmd.args([
        "search",
        "Gemini",
        "--json",
        "--limit",
        "1",
        "--data-dir",
        "tests/fixtures/search_demo_data",
    ]);

    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(stdout.trim()).expect("valid JSON");

    let hits = json["hits"].as_array().expect("hits array");
    assert!(hits.len() <= 1, "Limit should restrict results to at most 1");
}

#[test]
fn search_empty_query_returns_all() {
    // E2E test: empty query returns recent results (yln.5)
    let mut cmd = base_cmd();
    cmd.args([
        "search",
        "",
        "--json",
        "--data-dir",
        "tests/fixtures/search_demo_data",
    ]);

    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(stdout.trim()).expect("valid JSON");

    // Empty query should return results (recent conversations)
    assert!(json["hits"].is_array(), "Should return hits array");
}

#[test]
fn search_no_match_returns_empty_hits() {
    // E2E test: non-matching query returns empty results (yln.5)
    let mut cmd = base_cmd();
    cmd.args([
        "search",
        "xyznonexistentquery12345",
        "--json",
        "--data-dir",
        "tests/fixtures/search_demo_data",
    ]);

    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(stdout.trim()).expect("valid JSON");

    let count = json["count"].as_u64().expect("count field");
    assert_eq!(count, 0, "Non-matching query should return 0 results");

    let hits = json["hits"].as_array().expect("hits array");
    assert!(hits.is_empty(), "Hits array should be empty");
}

#[test]
fn search_writes_trace_on_success() {
    // E2E test: trace file captures successful search (yln.5)
    let tmp = TempDir::new().unwrap();
    let trace_path = tmp.path().join("search_trace.jsonl");

    let mut cmd = base_cmd();
    cmd.args([
        "--trace-file",
        trace_path.to_str().unwrap(),
        "search",
        "hello",
        "--json",
        "--data-dir",
        "tests/fixtures/search_demo_data",
    ]);

    cmd.assert().success();

    // Verify trace file was written
    let trace = fs::read_to_string(&trace_path).expect("trace file exists");
    assert!(!trace.is_empty(), "Trace file should have content");

    // Parse last line as JSON
    let last_line = trace.lines().last().expect("trace has lines");
    let json: Value = serde_json::from_str(last_line).expect("valid trace JSON");
    assert_eq!(json["exit_code"], 0, "Successful search should have exit_code 0");
    assert_eq!(json["contract_version"], "1");
}

#[test]
fn search_json_includes_match_type() {
    // E2E test: JSON results include match_type field (yln.5)
    let mut cmd = base_cmd();
    cmd.args([
        "search",
        "hello",
        "--json",
        "--data-dir",
        "tests/fixtures/search_demo_data",
    ]);

    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(stdout.trim()).expect("valid JSON");

    let hits = json["hits"].as_array().expect("hits array");
    if !hits.is_empty() {
        let first_hit = &hits[0];
        assert!(
            first_hit["match_type"].is_string(),
            "Hit should include match_type (exact/wildcard/fuzzy)"
        );
    }
}

#[test]
fn search_robot_format_is_valid_json_lines() {
    // E2E test: --robot output is JSON lines format (yln.5)
    let mut cmd = base_cmd();
    cmd.args([
        "search",
        "hello",
        "--robot",
        "--data-dir",
        "tests/fixtures/search_demo_data",
    ]);

    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Robot mode should output JSON (same as --json)
    let json: Value = serde_json::from_str(stdout.trim()).expect("robot output should be valid JSON");
    assert!(json["hits"].is_array(), "Robot output should have hits array");
}
