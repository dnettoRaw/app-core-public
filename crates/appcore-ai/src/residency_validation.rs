// =============================================================================
//        #######
//     ###       ###     F: residency_validation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/22 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/22 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{AiError, AiResult, ResidencyConfig, ResidencyRecord, ResidencyRequest};
use std::collections::BTreeSet;

pub(crate) fn validate_request(
    request: &ResidencyRequest,
    config: ResidencyConfig,
) -> AiResult<()> {
    if request.size_bytes == 0
        || request.importance_basis_points > 10_000
        || request.fallbacks.len() > config.max_fallback_tiers
        || (request.prefetch && request.size_bytes > config.max_prefetch_bytes)
    {
        return Err(AiError::InvalidInput("residency request"));
    }
    let mut tiers = BTreeSet::new();
    tiers.insert(request.preferred.clone());
    if request
        .fallbacks
        .iter()
        .any(|tier| !tiers.insert(tier.clone()))
    {
        return Err(AiError::InvalidInput("residency fallback tier"));
    }
    Ok(())
}

pub(crate) fn validate_record(record: &ResidencyRecord) -> AiResult<()> {
    if record.size_bytes == 0 || record.importance_basis_points > 10_000 {
        return Err(AiError::InvalidInput("residency record"));
    }
    Ok(())
}
