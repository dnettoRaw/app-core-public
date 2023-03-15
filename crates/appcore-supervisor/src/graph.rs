// =============================================================================
//        #######
//     ###       ###     F: graph.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 13:18:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Managed-service dependency graph validation and ordering.

use crate::{DependencyRequirement, ManagedService, SupervisorError, SupervisorResult};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub(crate) fn validate_dependencies(
    services: &BTreeMap<String, Arc<dyn ManagedService>>,
) -> SupervisorResult<()> {
    for (name, service) in services {
        for dependency in service.descriptor().dependencies() {
            if !services.contains_key(dependency.service_id())
                && dependency.requirement() != DependencyRequirement::Optional
            {
                return Err(SupervisorError::DependencyNotFound {
                    service: name.clone(),
                    dependency: dependency.service_id().to_string(),
                });
            }
        }
    }
    Ok(())
}

pub(crate) fn topological_order(
    services: &BTreeMap<String, Arc<dyn ManagedService>>,
) -> SupervisorResult<Vec<String>> {
    let mut temporary = BTreeSet::new();
    let mut permanent = BTreeSet::new();
    let mut order = Vec::with_capacity(services.len());
    for name in services.keys() {
        visit(
            name,
            services,
            &mut temporary,
            &mut permanent,
            &mut order,
            &mut Vec::new(),
        )?;
    }
    Ok(order)
}

fn visit(
    name: &str,
    services: &BTreeMap<String, Arc<dyn ManagedService>>,
    temporary: &mut BTreeSet<String>,
    permanent: &mut BTreeSet<String>,
    order: &mut Vec<String>,
    path: &mut Vec<String>,
) -> SupervisorResult<()> {
    if permanent.contains(name) {
        return Ok(());
    }
    if !temporary.insert(name.to_string()) {
        path.push(name.to_string());
        return Err(SupervisorError::DependencyCycle(path.clone()));
    }
    path.push(name.to_string());
    let service = services
        .get(name)
        .ok_or_else(|| SupervisorError::ServiceNotFound(name.to_string()))?;
    for dependency in service.descriptor().dependencies() {
        if services.contains_key(dependency.service_id()) {
            visit(
                dependency.service_id(),
                services,
                temporary,
                permanent,
                order,
                path,
            )?;
        }
    }
    path.pop();
    temporary.remove(name);
    permanent.insert(name.to_string());
    order.push(name.to_string());
    Ok(())
}
