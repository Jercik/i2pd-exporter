use serde_json as json;

#[test]
fn routerinfo_full_golden() {
    let json_str = include_str!("fixtures/routerinfo_full.json");
    let expected_router_only = include_str!("fixtures/routerinfo_full.prom");

    let data: i2pd_exporter::i2pcontrol::types::RouterInfoResult =
        json::from_str(json_str).expect("valid RouterInfoResult JSON");

    // Generate full metrics text (router + exporter self-metrics)
    let got = i2pd_exporter::metrics::encode_metrics_text(
        Some(&data),
        0.0,
        None,
        0,
        i2pd_exporter::version::VERSION,
    );

    // Debug output for troubleshooting differences
    eprintln!("{}", got);

    // Ensure every expected router metric line appears in the encoded output
    for line in expected_router_only
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
    {
        let mut ok = got.contains(line);

        // Accept trimmed float representation differences (e.g., 0.870000 vs 0.87)
        if !ok {
            if let Some((head, val)) = line.rsplit_once(' ') {
                if let Ok(f) = val.parse::<f64>() {
                    let alt = format!("{} {}", head, f);
                    ok = ok || got.contains(&alt);
                }
            }
        }

        assert!(ok, "missing expected line in output: {}", line);
    }
}
