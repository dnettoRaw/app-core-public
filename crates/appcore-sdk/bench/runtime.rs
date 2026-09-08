// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/05 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/05 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Measures facade construction and actual immutable application preparation.

#[cfg(feature = "api")]
use appcore_sdk::application::{ApiRequest, ApiResponse, ApiRouter, QueryEndpoint, QueryName};
#[cfg(feature = "scheduler")]
use appcore_sdk::application::{ApplicationTaskRegistry, RetryPolicy, ScheduledTask, TaskSchedule};
use appcore_sdk::application::{
    CommandBus, CommandEnvelope, CommandHandler, CommandName, CommandRegistry, CommandResult,
    DecisionEngine, DecisionNode, DecisionOutcome, DecisionRegistry, EventName, EventRegistry,
    NodeId, RuntimeContext, RuntimeResult, StateName, StateRegistry,
};
use appcore_sdk::{App, AppResult, Application};
use std::hint::black_box;
use std::time::{Duration, Instant};

struct RegisteredApp(usize);

#[cfg(all(feature = "api", feature = "scheduler"))]
struct FullRegisteredApp(usize);

#[cfg(feature = "api")]
struct EchoQuery(QueryName);

#[cfg(feature = "api")]
impl QueryEndpoint for EchoQuery {
    fn query_name(&self) -> &QueryName {
        &self.0
    }

    fn handle_query(&self, request: ApiRequest) -> RuntimeResult<ApiResponse> {
        Ok(ApiResponse {
            status_code: 200,
            payload: request.payload,
        })
    }
}

struct AllowDecision(String);

impl DecisionNode for AllowDecision {
    fn name(&self) -> &str {
        &self.0
    }

    fn decide(
        &self,
        _command: &CommandEnvelope,
        _context: &dyn RuntimeContext,
    ) -> RuntimeResult<DecisionOutcome> {
        Ok(DecisionOutcome::Allow)
    }
}

struct AcceptHandler(CommandName);

impl CommandHandler for AcceptHandler {
    fn command_name(&self) -> CommandName {
        self.0.clone()
    }

    fn handle(
        &self,
        _command: &CommandEnvelope,
        _context: &dyn RuntimeContext,
    ) -> RuntimeResult<CommandResult> {
        Ok(CommandResult::accepted(Vec::new()))
    }
}

impl Application for RegisteredApp {
    fn register_commands(&self, registry: &mut CommandRegistry) -> RuntimeResult<()> {
        for index in 0..self.0 {
            registry.register(CommandName::new(format!("benchmark.operation_{index}"))?)?;
        }
        Ok(())
    }

    fn register_events(&self, registry: &mut EventRegistry) -> RuntimeResult<()> {
        for index in 0..self.0 {
            registry.register(EventName::new(format!("BenchmarkEvent{index}"))?)?;
        }
        Ok(())
    }

    fn register_states(&self, registry: &mut StateRegistry) -> RuntimeResult<()> {
        for index in 0..self.0 {
            registry.register(StateName::new(format!("BenchmarkState{index}"))?)?;
        }
        Ok(())
    }

    fn register_decisions(&self, registry: &mut DecisionRegistry) -> RuntimeResult<()> {
        for index in 0..self.0 {
            registry.register_name(&format!("benchmark_decision_{index}"))?;
        }
        Ok(())
    }

    fn register_decision_nodes(&self, engine: &mut DecisionEngine) -> RuntimeResult<()> {
        for index in 0..self.0 {
            engine.register_node(AllowDecision(format!("benchmark_decision_{index}")))?;
        }
        Ok(())
    }

    fn register_handlers(&self, bus: &mut CommandBus) -> RuntimeResult<()> {
        for index in 0..self.0 {
            bus.register_handler(AcceptHandler(CommandName::new(format!(
                "benchmark.operation_{index}"
            ))?))?;
        }
        Ok(())
    }
}

#[cfg(all(feature = "api", feature = "scheduler"))]
impl Application for FullRegisteredApp {
    fn register_commands(&self, registry: &mut CommandRegistry) -> RuntimeResult<()> {
        RegisteredApp(self.0).register_commands(registry)
    }

    fn register_events(&self, registry: &mut EventRegistry) -> RuntimeResult<()> {
        RegisteredApp(self.0).register_events(registry)
    }

    fn register_states(&self, registry: &mut StateRegistry) -> RuntimeResult<()> {
        RegisteredApp(self.0).register_states(registry)
    }

    fn register_decisions(&self, registry: &mut DecisionRegistry) -> RuntimeResult<()> {
        RegisteredApp(self.0).register_decisions(registry)
    }

    fn register_decision_nodes(&self, engine: &mut DecisionEngine) -> RuntimeResult<()> {
        RegisteredApp(self.0).register_decision_nodes(engine)
    }

    fn register_handlers(&self, bus: &mut CommandBus) -> RuntimeResult<()> {
        RegisteredApp(self.0).register_handlers(bus)
    }

    fn register_queries(&self, router: &mut ApiRouter) -> RuntimeResult<()> {
        for index in 0..self.0 {
            router.register_query(EchoQuery(QueryName::new(format!(
                "benchmark.query_{index}"
            ))?))?;
        }
        Ok(())
    }

    fn register_tasks(&self, registry: &mut ApplicationTaskRegistry) -> RuntimeResult<()> {
        for index in 0..self.0 {
            registry.register(
                ScheduledTask {
                    id: format!("benchmark.task_{index}"),
                    schedule: TaskSchedule::Once {
                        run_at: std::time::SystemTime::UNIX_EPOCH,
                    },
                    retry: RetryPolicy::default(),
                    priority: 0,
                    trace: None,
                },
                |_| Ok(()),
            )?;
        }
        Ok(())
    }
}

fn main() -> AppResult<()> {
    memory_checkpoint("idle");
    let iterations = std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| (1..=1_000_000_000).contains(value))
        .unwrap_or(1000);
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    for (name, commands) in [
        ("construct_defaults", None),
        ("prepare_empty", Some(0)),
        ("prepare_32_core_hooks", Some(32)),
    ] {
        if selected.as_deref().is_some_and(|value| value != name) {
            continue;
        }
        memory_checkpoint("workload");
        let started = Instant::now();
        for _ in 0..iterations {
            let app = App::new("benchmark")?;
            if let Some(count) = commands {
                let prepared =
                    app.prepare(&RegisteredApp(count), NodeId::new("benchmark-local")?)?;
                assert_eq!(prepared.runtime().commands().len(), count);
                assert_eq!(prepared.runtime().events().len(), count);
                assert_eq!(prepared.runtime().states().len(), count);
                assert_eq!(prepared.runtime().decisions().len(), count);
                assert_eq!(prepared.runtime().command_bus().len(), count);
                black_box(prepared);
            } else {
                black_box(app);
            }
        }
        let total_ns = started.elapsed().as_nanos();
        println!(
            "appcore-sdk::{name} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
            total_ns as f64 / iterations as f64
        );
        memory_checkpoint("retained");
    }
    #[cfg(all(feature = "api", feature = "scheduler"))]
    benchmark_full_hooks(iterations, selected.as_deref())?;
    Ok(())
}

#[cfg(all(feature = "api", feature = "scheduler"))]
fn benchmark_full_hooks(iterations: u64, selected: Option<&str>) -> AppResult<()> {
    const NAME: &str = "prepare_32_full_hooks";
    if selected.is_some_and(|value| value != NAME) {
        return Ok(());
    }
    memory_checkpoint("workload");
    let started = Instant::now();
    for _ in 0..iterations {
        let prepared = App::new("benchmark")?
            .prepare(&FullRegisteredApp(32), NodeId::new("benchmark-local")?)?;
        assert_eq!(prepared.runtime().commands().len(), 32);
        assert_eq!(prepared.queries().query_names_iter().len(), 32);
        assert_eq!(prepared.tasks().len(), 32);
        black_box(prepared);
    }
    let total_ns = started.elapsed().as_nanos();
    println!(
        "appcore-sdk::{NAME} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    memory_checkpoint("retained");
    Ok(())
}

fn memory_checkpoint(phase: &str) {
    let Some(milliseconds) = std::env::var("APPCORE_BENCH_MEMORY_CHECKPOINT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| (1..=1000).contains(value))
    else {
        return;
    };
    println!(
        "appcore-bench-memory phase={phase} pid={}",
        std::process::id()
    );
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::thread::sleep(Duration::from_millis(milliseconds));
}
