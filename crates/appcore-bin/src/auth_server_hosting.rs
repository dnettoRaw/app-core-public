// =============================================================================
//        #######
//     ###       ###     F: auth_server_hosting.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 13:18:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Auth Server hosting, Supervisor ownership, and monitor lifecycle.

use super::*;
use appcore_supervisor::{ManagedService, PassiveManagedService, ServiceHealth, WatchdogState};

pub(crate) fn run_auth_server_serve(options: AuthServerServeOptions) -> Result<(), String> {
    run_auth_server_with_hosting(options, AuthServerHosting::StandaloneCompanion)
}

fn run_auth_server_with_hosting(
    options: AuthServerServeOptions,
    hosting: AuthServerHosting,
) -> Result<(), String> {
    install_auth_ctrlc_handler()?;
    AUTH_STOP_REQUESTED.store(false, Ordering::Release);
    let (supervisor, standalone) = supervisor_for_hosting(hosting)?;
    let transport = load_provider(Path::new(&options.transport_secret_path), "transport")?;
    let data = load_provider(Path::new(&options.data_secret_path), "data")?;
    let state = AuthHttpState {
        transport,
        data,
        replay: Arc::new(AuthReplayCache::default()),
        concurrency: Arc::new(Semaphore::new(AUTH_MAX_CONCURRENCY)),
        rate_limit: Arc::new(RateLimiter::new(AUTH_RATE_LIMIT_PER_SECOND, 1_000)),
        timeout: AUTH_REQUEST_TIMEOUT,
        supervisor: Arc::clone(&supervisor),
    };
    let replay_metrics = Arc::clone(&state.replay);
    let service = auth_managed_service(&options, state)?;
    start_hosted_auth_service(&supervisor, service, standalone)?;
    println!("auth_server: listening {}", options.bind);
    println!("auto_restart: {}", options.auto_restart);
    let watchdog = standalone
        .then(|| start_auth_watchdog(Arc::clone(&supervisor)))
        .transpose()?;
    let monitor_result = monitor_auth_service(&supervisor, standalone);
    AUTH_STOP_REQUESTED.store(true, Ordering::Release);
    join_watchdog(watchdog)?;
    stop_hosted_auth_service(&supervisor, standalone)?;
    monitor_result?;
    print_replay_metrics(replay_metrics.metrics());
    Ok(())
}

pub(super) fn start_hosted_auth_service(
    supervisor: &Supervisor,
    service: Arc<dyn ManagedService>,
    standalone: bool,
) -> Result<(), String> {
    supervisor
        .register_or_replace_inactive(service)
        .map_err(|error| error.to_string())?;
    if standalone {
        supervisor.start_all()
    } else {
        supervisor.start("auth-server", now_ms())
    }
    .map_err(|error| error.to_string())
}

pub(super) fn supervisor_for_hosting(
    hosting: AuthServerHosting,
) -> Result<(Arc<Supervisor>, bool), String> {
    match hosting {
        AuthServerHosting::RuntimeManaged(supervisor) => Ok((supervisor, false)),
        AuthServerHosting::StandaloneCompanion => {
            let supervisor = Arc::new(Supervisor::new());
            let security = ServiceDescriptor::new(
                "security",
                ManagedResource::Security,
                RestartPolicy::never(),
            )
            .map(PassiveManagedService::new)
            .map(Arc::new)
            .map_err(|error| error.to_string())?;
            supervisor
                .register(security)
                .map_err(|error| error.to_string())?;
            Ok((supervisor, true))
        }
    }
}

fn monitor_auth_service(supervisor: &Supervisor, reconcile_locally: bool) -> Result<(), String> {
    while !AUTH_STOP_REQUESTED.load(Ordering::Acquire) {
        if reconcile_locally {
            supervisor
                .reconcile(now_ms())
                .map_err(|error| error.to_string())?;
        } else if supervisor.watchdog().state() == WatchdogState::Stopping {
            return Ok(());
        }
        let failed = supervisor.snapshots().iter().any(|snapshot| {
            snapshot.name == "auth-server"
                && snapshot.health == ServiceHealth::Failed
                && (snapshot.operator_required || snapshot.restart_count == 0)
        });
        if failed {
            return Err("auth server failed and requires operator action".to_string());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn start_auth_watchdog(supervisor: Arc<Supervisor>) -> Result<thread::JoinHandle<()>, String> {
    let interval = Duration::from_millis(supervisor.watchdog().config().check_interval_ms.max(1));
    thread::Builder::new()
        .name("appcore-auth-watchdog".to_string())
        .spawn(move || {
            while !AUTH_STOP_REQUESTED.load(Ordering::Acquire) {
                let _ = supervisor.evaluate_watchdog(now_ms());
                thread::sleep(interval);
            }
        })
        .map_err(|error| error.to_string())
}

fn join_watchdog(watchdog: Option<thread::JoinHandle<()>>) -> Result<(), String> {
    watchdog.map_or(Ok(()), |watchdog| {
        watchdog
            .join()
            .map_err(|_| "auth watchdog panicked".to_string())
    })
}

fn stop_hosted_auth_service(supervisor: &Supervisor, standalone: bool) -> Result<(), String> {
    if standalone {
        supervisor.shutdown(now_ms())
    } else {
        supervisor.stop("auth-server", now_ms())
    }
    .map_err(|error| error.to_string())
}

fn print_replay_metrics(metrics: ReplayStoreMetrics) {
    println!(
        "replay_store: entries={} accepted={} replays={} expired={} capacity_rejections={}",
        metrics.entries,
        metrics.accepted,
        metrics.replays,
        metrics.expired,
        metrics.capacity_rejections
    );
}
