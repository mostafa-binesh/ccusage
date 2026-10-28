use crate::{LoadedEntry, PricingMap, Result, cli::SharedArgs};

use super::parser;

pub fn load_entries(shared: &SharedArgs, pricing: &PricingMap) -> Result<Vec<LoadedEntry>> {
    crate::progress::track_usage_load(
        crate::progress::UsageLoadAgent("Qoder"),
        shared.json,
        || parser::load_entries(shared, pricing),
    )
}
