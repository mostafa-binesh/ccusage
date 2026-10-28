# Qoder Data Source (Experimental)

> Qoder support is experimental because the application log format is not a documented public API.

ccusage reads Qoder's local session event logs and reports the token usage that Qoder records for completed model responses.

## Focused Views

::: code-group

```bash [bunx (Recommended)]
bunx ccusage qoder --help
```

```bash [npx]
npx ccusage@latest qoder --help
```

```bash [pnpm]
pnpm dlx ccusage qoder --help
```

:::

## Data Source

The CLI reads JSONL session segments from `QODER_DATA_DIR`, which defaults to `~/.qoder`. Set it to one root or a comma-separated list of roots.

```bash
QODER_DATA_DIR="$HOME/.qoder,/backup/qoder" ccusage qoder daily
```

```text
~/.qoder/
└── logs/
    └── sessions/
        └── {project}/
            └── {session}/
                └── segments/
                    └── *.jsonl
```

Only `model.response.completed` events are counted. Their `data` payload provides `input_tokens`, `output_tokens`, `cache_read_input_tokens`, and `cache_creation_input_tokens`.

## Report Views

| Focused view            | Description                  | See also                                |
| ----------------------- | ---------------------------- | --------------------------------------- |
| `ccusage qoder daily`   | Aggregate usage by date      | [Daily Usage](/guide/daily-reports)     |
| `ccusage qoder monthly` | Aggregate usage by month     | [Monthly Usage](/guide/monthly-reports) |
| `ccusage qoder session` | Group usage by Qoder session | [Session Usage](/guide/session-reports) |

These views support `--json`, `--compact`, and `--offline`.

## Token and Cost Handling

- **Token usage** - The adapter uses the four token fields written on completed model responses.
- **Zero placeholders** - Events where every token field is zero are not counted. This prevents logs that lack token accounting from being reported as real usage.
- **Pricing** - Costs use the raw Qoder model name with ccusage's normal pricing table and any configured pricing override. A model without a matching price remains $0.00.

## Environment Variables

| Variable         | Description                                                                            |
| ---------------- | -------------------------------------------------------------------------------------- |
| `QODER_DATA_DIR` | Override the root directory, or comma-separated root directories, containing Qoder data |
| `LOG_LEVEL`      | Adjust verbosity (0 silent ... 5 trace)                                                |

## Troubleshooting

::: details No Qoder usage data found
Ensure that `~/.qoder/logs/sessions/{project}/{session}/segments/` contains Qoder JSONL session segments. Set `QODER_DATA_DIR` when the data lives in a different or archived root.
:::

::: details Qoder is detected but totals are zero
Some Qoder versions write zero-valued token fields. ccusage deliberately excludes those placeholders because it cannot recover actual token counts from them. Update Qoder or use a version that records non-zero token accounting.
:::