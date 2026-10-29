# ccusage-adapter-qoder

The Qoder adapter reads Qoder session event logs and turns completed model responses into ccusage usage entries.

## Data source

- `${QODER_DATA_DIR:-~/.qoder}/logs/sessions/{project}/{session}/segments/*.jsonl`

Each `model.response.completed` event provides the model and input, output, cache-read, and cache-creation token counts. Records with every token count at zero are skipped: Qoder has persisted those placeholder values in some releases, and treating them as usage would produce false totals.

For session events with placeholder token counts, the adapter reads `${QODER_DATA_DIR:-~/.qoder}/projects/**/*.jsonl` for the matching per-response `credits` value. It does not add a credit fallback where Qoder supplied exact tokens. This fallback does not fabricate token counts or convert Qoder credits to USD; Qoder does not store enough information locally to do either reliably.

`QODER_DATA_DIR` can contain one root or a comma-separated list of roots. The adapter reads plain JSONL files through `ccusage-adapter-common`, which handles parallel file reads.

## Public surface

- `loader::load_entries`
- `report::report_from_rows`
- `report::summarize_entries`
- `run`
- `has_data`
