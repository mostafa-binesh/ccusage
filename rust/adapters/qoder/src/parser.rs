use std::{
    collections::HashSet,
    fs,
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use jiff::tz::TimeZone as JiffTimeZone;
use serde::Deserialize;

use ccusage_adapter_common::jsonl;

use super::paths;
use crate::{
    LoadedEntry, PricingMap, Result, TimestampMs, TokenUsageRaw, UsageEntry, UsageMessage,
    calculate_cost_for_usage_at,
    cli::{CostMode, SharedArgs},
    debug_log, format_date_tz, format_rfc3339_millis, missing_pricing_model_for_candidates,
    parse_ts_timestamp, parse_tz, read_files_parallel, total_usage_tokens,
};

const RESPONSE_COMPLETED: &str = "model.response.completed";
const DEFAULT_QODER_MODEL: &str = "unknown";

/// Qoder records token accounting in completed-model-response event payloads.
#[derive(Debug, Deserialize)]
struct QoderLine {
    #[serde(default, deserialize_with = "jsonl::non_empty_string")]
    r#type: Option<String>,
    #[serde(default, deserialize_with = "jsonl::non_empty_string")]
    ts: Option<String>,
    #[serde(default, deserialize_with = "jsonl::lenient_u64")]
    seq: u64,
    data: Option<QoderUsage>,
}

#[derive(Debug, Default, Deserialize)]
struct QoderUsage {
    #[serde(default, deserialize_with = "jsonl::non_empty_string")]
    model: Option<String>,
    #[serde(default, deserialize_with = "jsonl::lenient_u64")]
    input_tokens: u64,
    #[serde(default, deserialize_with = "jsonl::lenient_u64")]
    output_tokens: u64,
    #[serde(default, deserialize_with = "jsonl::lenient_u64")]
    cache_read_input_tokens: u64,
    #[serde(default, deserialize_with = "jsonl::lenient_u64")]
    cache_creation_input_tokens: u64,
}

pub(super) fn load_entries(shared: &SharedArgs, pricing: &PricingMap) -> Result<Vec<LoadedEntry>> {
    let tz = parse_tz(shared.timezone.as_deref());
    let files = paths::discover_session_files()?;
    let loaded = read_files_parallel(&files, shared.single_thread, |file| {
        read_session_file(file, tz.as_ref(), shared.mode, pricing, shared).unwrap_or_else(|error| {
            debug_log(
                shared,
                format!(
                    "Failed to read Qoder session file {}: {error}",
                    file.display()
                ),
            );
            Vec::new()
        })
    });
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    for file_entries in loaded {
        for entry in file_entries {
            if seen.insert(entry_id(&entry)) {
                entries.push(entry);
            }
        }
    }
    entries.sort_by_key(|entry| entry.timestamp);
    Ok(entries)
}

fn read_session_file(
    file: &Path,
    tz: Option<&JiffTimeZone>,
    mode: CostMode,
    pricing: &PricingMap,
    shared: &SharedArgs,
) -> Result<Vec<LoadedEntry>> {
    let fallback = file_timestamp(file, shared);
    let content = fs::read(file)?;
    let mut entries = Vec::new();
    for record in jsonl::records::<QoderLine>(&content, None) {
        if let Some(entry) = parse_line(file, fallback, &record, tz, mode, pricing) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

fn parse_line(
    file: &Path,
    fallback: TimestampMs,
    record: &QoderLine,
    tz: Option<&JiffTimeZone>,
    mode: CostMode,
    pricing: &PricingMap,
) -> Option<LoadedEntry> {
    if record.r#type.as_deref() != Some(RESPONSE_COMPLETED) {
        return None;
    }
    let raw_usage = record.data.as_ref()?;
    let usage = TokenUsageRaw {
        input_tokens: raw_usage.input_tokens,
        output_tokens: raw_usage.output_tokens,
        cache_creation_input_tokens: raw_usage.cache_creation_input_tokens,
        cache_read_input_tokens: raw_usage.cache_read_input_tokens,
        speed: None,
        cache_creation: None,
    };
    if total_usage_tokens(usage) == 0 {
        return None;
    }

    let timestamp_text = record
        .ts
        .clone()
        .and_then(|value| parse_ts_timestamp(&value).map(|_| value))
        .unwrap_or_else(|| format_rfc3339_millis(fallback));
    let timestamp = parse_ts_timestamp(&timestamp_text).unwrap_or(fallback);
    let (project, session_id) = paths::session_from_file(file)?;
    let model = raw_usage
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_QODER_MODEL.to_string());
    let cost = calculate_cost_for_usage_at(
        Some(&model),
        usage,
        None,
        Some(timestamp),
        mode,
        Some(pricing),
    );
    let missing_pricing_model = if mode == CostMode::Display {
        None
    } else {
        missing_pricing_model_for_candidates(
            &model,
            [model.clone()],
            total_usage_tokens(usage),
            Some(pricing),
        )
    };
    let data = UsageEntry {
        session_id: Some(session_id.clone()),
        timestamp: timestamp_text,
        version: None,
        message: UsageMessage {
            usage,
            model: Some(model.clone()),
            id: (record.seq != 0).then(|| record.seq.to_string()),
        },
        cost_usd: None,
        request_id: None,
        is_api_error_message: None,
        is_sidechain: None,
    };
    Some(LoadedEntry {
        data,
        timestamp,
        date: format_date_tz(timestamp, tz),
        project: Arc::from("qoder"),
        session_id: Arc::from(session_id),
        project_path: Arc::from(project),
        cost,
        credits: None,
        model: Some(model),
        message_count: None,
        usage_limit_reset_time: None,
        missing_pricing_model,
        extra_total_tokens: 0,
        stored_cost: None,
    })
}

fn file_timestamp(file: &Path, shared: &SharedArgs) -> TimestampMs {
    match fs::metadata(file)
        .and_then(|metadata| metadata.modified())
        .and_then(|modified| {
            modified
                .duration_since(UNIX_EPOCH)
                .map_err(std::io::Error::other)
        }) {
        Ok(duration) => TimestampMs::from_millis(duration.as_millis().min(i64::MAX as u128) as i64),
        Err(error) => {
            debug_log(
                shared,
                format!(
                    "Failed to read Qoder session file timestamp for {}: {error}",
                    file.display()
                ),
            );
            system_time_timestamp(SystemTime::now())
        }
    }
}

fn system_time_timestamp(time: SystemTime) -> TimestampMs {
    time.duration_since(UNIX_EPOCH).map_or_else(
        |_| TimestampMs::UNIX_EPOCH,
        |duration| TimestampMs::from_millis(duration.as_millis().min(i64::MAX as u128) as i64),
    )
}

fn entry_id(entry: &LoadedEntry) -> String {
    serde_json::json!([
        entry.session_id.as_ref(),
        entry.data.message.id,
        entry.data.timestamp,
        entry.data.message.model,
    ])
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_completed_qoder_response_tokens() {
        let record = serde_json::from_value::<QoderLine>(serde_json::json!({
            "type": "model.response.completed",
            "ts": "2026-10-28T00:38:57.351+03:30",
            "seq": 91,
            "data": {
                "model": "ultimate",
                "input_tokens": 100,
                "output_tokens": 50,
                "cache_read_input_tokens": 20,
                "cache_creation_input_tokens": 30
            }
        }))
        .unwrap();
        let entry = parse_line(
            Path::new("/tmp/.qoder/logs/sessions/project-a/session-a/segments/segment.jsonl"),
            TimestampMs::UNIX_EPOCH,
            &record,
            Some(&JiffTimeZone::UTC),
            CostMode::Display,
            &PricingMap::default(),
        )
        .unwrap();

        assert_eq!(entry.date, "2026-10-27");
        assert_eq!(entry.session_id.as_ref(), "session-a");
        assert_eq!(entry.project_path.as_ref(), "project-a");
        assert_eq!(entry.data.message.model.as_deref(), Some("ultimate"));
        assert_eq!(entry.data.message.usage.input_tokens, 100);
        assert_eq!(entry.data.message.usage.output_tokens, 50);
        assert_eq!(entry.data.message.usage.cache_read_input_tokens, 20);
        assert_eq!(entry.data.message.usage.cache_creation_input_tokens, 30);
    }

    #[test]
    fn ignores_zero_token_qoder_responses() {
        let record = serde_json::from_value::<QoderLine>(serde_json::json!({
            "type": "model.response.completed",
            "ts": "2026-10-28T00:38:57.351+03:30",
            "data": { "model": "ultimate" }
        }))
        .unwrap();

        assert!(
            parse_line(
                Path::new("/tmp/.qoder/logs/sessions/project-a/session-a/segments/segment.jsonl"),
                TimestampMs::UNIX_EPOCH,
                &record,
                None,
                CostMode::Display,
                &PricingMap::default(),
            )
            .is_none()
        );
    }
}
