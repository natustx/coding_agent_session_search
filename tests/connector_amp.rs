use coding_agent_search::connectors::amp::AmpConnector;
use coding_agent_search::connectors::{Connector, ScanContext};
use std::path::PathBuf;

#[test]
fn amp_parses_minimal_cache() {
    let fixture_root = PathBuf::from("tests/fixtures/amp");
    let conn = AmpConnector::new();
    let _detect = conn.detect();
    // Detection may fail on systems without amp cache; force scan with our fixture root.
    let ctx = ScanContext {
        data_root: fixture_root.clone(),
        since_ts: None,
    };
    let convs = conn.scan(&ctx).expect("scan");
    assert!(!convs.is_empty(), "expected at least one conversation");
    let c = &convs[0];
    assert_eq!(c.agent_slug, "amp");
    assert!(c.external_id.is_some());
    assert!(!c.messages.is_empty());
}
