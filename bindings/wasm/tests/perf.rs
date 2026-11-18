//! Kition Doc's performance budgets (Kition `docs/kits.md` section 7), measured by the
//! convergence suite's perf mode. Run in release:
//! `cargo test --release -p kition-doc-kit --test perf -- --ignored --nocapture`.

mod common;

use common::DocKit;
use kition_convergence::CountingAlloc;
use kition_convergence::perf::{self, PerfOptions};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

#[test]
#[ignore = "a measurement, run in release"]
fn budgets() {
    let report = perf::measure(&DocKit, &PerfOptions { allocator: Some(&ALLOC), ..PerfOptions::default() }).unwrap();
    println!("{}", report.render());
}
