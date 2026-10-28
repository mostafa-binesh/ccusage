use ccusage_adapter_common::{
    collect_files_with_extension, filter_loaded_entries_by_date, read_files_parallel,
};
use ccusage_core::*;

mod loader;
mod parser;
mod paths;
mod report;

use crate::{
    Result,
    cli::{AgentCommandArgs, AgentReportKind, SharedArgs},
    print_json_or_jq, print_usage_table, sort_summaries, wants_json,
};

pub use loader::load_entries;
pub(crate) use report::report_from_rows;
pub use report::summarize_entries;

pub fn run(args: AgentCommandArgs) -> Result<()> {
    let pricing = PricingMap::load_with_overrides(
        args.shared.offline,
        crate::log_level() != Some(0),
        args.shared.pricing_overrides.iter(),
    );
    let mut entries = load_entries(&args.shared, &pricing)?;
    let mut rows = if args.kind == AgentReportKind::Session {
        summarize_entries(&entries, args.kind)?
    } else {
        filter_loaded_entries_by_date(&mut entries, &args.shared);
        summarize_entries(&entries, args.kind)?
    };
    if args.kind == AgentReportKind::Session {
        filter_session_summaries(&mut rows, &args.shared);
    }
    sort_summaries(&mut rows, &args.shared.order, |row| {
        ccusage_core::summary_period(row)
    });
    if wants_json(&args.shared) {
        return print_json_or_jq(
            report_from_rows(&rows, args.kind),
            args.shared.jq.as_deref(),
            args.shared.no_cost,
        );
    }
    print_usage_table(
        "Qoder Token Usage Report",
        ccusage_core::first_column(args.kind),
        &rows,
        &args.shared,
        false,
        None,
    )?;
    Ok(())
}

fn filter_session_summaries(rows: &mut Vec<crate::UsageSummary>, shared: &SharedArgs) {
    if shared.since.is_some() || shared.until.is_some() {
        let since = shared.since.as_deref().map(|value| value.replace('-', ""));
        let until = shared.until.as_deref().map(|value| value.replace('-', ""));
        rows.retain(|row| {
            let date = row
                .last_activity
                .as_deref()
                .unwrap_or_default()
                .replace('-', "");
            since.as_ref().is_none_or(|bound| &date >= bound)
                && until.as_ref().is_none_or(|bound| &date <= bound)
        });
    }
}

pub fn has_data() -> bool {
    paths::discover_session_files().is_ok_and(|files| !files.is_empty())
}

#[cfg(test)]
mod tests {
    use ccusage_test_support::{EnvVarGuard, fs_fixture};

    use super::*;
    use crate::cli::{CostMode, SharedArgs};

    #[test]
    fn loads_qoder_session_usage_entries() {
        let fixture = fs_fixture!({
            "logs/sessions/project-a/session-a/segments/segment.jsonl": r#"{"type":"model.response.completed","ts":"2026-02-23T14:24:56.857Z","seq":1,"data":{"model":"qoder-model","input_tokens":100,"output_tokens":50,"cache_read_input_tokens":5,"cache_creation_input_tokens":10}}"#,
        });
        let shared = SharedArgs {
            mode: CostMode::Display,
            timezone: Some("UTC".to_string()),
            ..SharedArgs::default()
        };
        let _guard = EnvVarGuard::set("QODER_DATA_DIR", fixture.root());
        let entries = load_entries(&shared, &PricingMap::default()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].date, "2026-02-23");
        assert_eq!(entries[0].session_id.as_ref(), "session-a");
        assert_eq!(entries[0].project_path.as_ref(), "project-a");
        assert_eq!(entries[0].data.message.usage.input_tokens, 100);
        assert_eq!(entries[0].data.message.usage.output_tokens, 50);
        assert_eq!(entries[0].data.message.usage.cache_read_input_tokens, 5);
        assert_eq!(
            entries[0].data.message.usage.cache_creation_input_tokens,
            10
        );
    }
}
