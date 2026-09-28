//! 桌面启动时读取CPU/RAM，配置唯一图片线程池；不读取进程列表、不做负载轮询。
//! 固定池只决定并行上限，单图执行还必须取得核心工作集预算；这不是OS硬内存限额。

use crate::tasks::TaskConfig;
use pixofold_core::{batch::BatchConfig, model::ByteCount};
use sysinfo::{MemoryRefreshKind, System};

const MIB: u64 = 1024 * 1024;
const MAX_WORKERS: usize = 32;
const MAX_BUDGET: u64 = 4 * 1024 * MIB;
const WORKER_BASE_BUDGET: u64 = 128 * MIB;
const UNKNOWN_MEMORY_BUDGET: u64 = 256 * MIB;

pub(crate) fn task_config() -> TaskConfig {
    let cpu_threads = std::thread::available_parallelism().map_or(1, |value| value.get());
    let mut system = System::new();
    system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
    let batch = for_resources(
        cpu_threads,
        system.total_memory(),
        system.available_memory(),
    );
    tracing::info!(target: "pixofold", event = "worker_pool_configured",
        cpu_threads, workers = batch.workers, budget_bytes = batch.working_set_budget.0);
    TaskConfig {
        batch,
        ..TaskConfig::default()
    }
}

fn for_resources(cpu_threads: usize, total_bytes: u64, available_bytes: u64) -> BatchConfig {
    // total=0表示平台查询不支持/失败。已知但极低的可用内存不能被保底值抬高。
    if total_bytes == 0 {
        return BatchConfig {
            workers: 1,
            working_set_budget: ByteCount(UNKNOWN_MEMORY_BUDGET),
            ..BatchConfig::default()
        };
    }
    let budget = (available_bytes.min(total_bytes) / 2)
        .min(total_bytes / 4)
        .clamp(1, MAX_BUDGET);
    let memory_slots = (budget / WORKER_BASE_BUDGET).max(1) as usize;
    BatchConfig {
        workers: cpu_threads.clamp(1, MAX_WORKERS).min(memory_slots),
        working_set_budget: ByteCount(budget),
        ..BatchConfig::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_resource_probe_returns_a_valid_bounded_config() {
        let config = task_config();
        assert!((1..=MAX_WORKERS).contains(&config.batch.workers));
        assert!((1..=MAX_BUDGET).contains(&config.batch.working_set_budget.0));
        assert_eq!(config.batch.max_jobs, 1000);
    }

    #[test]
    fn cpu_memory_and_absolute_caps_all_bound_the_pool() {
        let cases = [
            (1, 32_768 * MIB, 16_384 * MIB, 1, 4096 * MIB),
            (8, 32_768 * MIB, 16_384 * MIB, 8, 4096 * MIB),
            (128, u64::MAX, u64::MAX, 32, 4096 * MIB),
            (16, 4096 * MIB, 1024 * MIB, 4, 512 * MIB),
            (16, 4096 * MIB, 4096 * MIB, 8, 1024 * MIB),
            (8, 1024 * MIB, 32 * MIB, 1, 16 * MIB),
            (0, 4096 * MIB, 1024 * MIB, 1, 512 * MIB),
            (32, 4096 * MIB, 0, 1, 1),
        ];
        for (cpu, total, available, workers, budget) in cases {
            let actual = for_resources(cpu, total, available);
            assert_eq!(
                (actual.workers, actual.working_set_budget.0),
                (workers, budget)
            );
            assert_eq!(actual.max_jobs, 1000);
        }
    }

    #[test]
    fn missing_memory_and_inconsistent_counters_fail_conservatively() {
        let missing = for_resources(32, 0, 8192 * MIB);
        assert_eq!(
            (missing.workers, missing.working_set_budget.0),
            (1, UNKNOWN_MEMORY_BUDGET)
        );
        let inconsistent = for_resources(16, 1024 * MIB, u64::MAX);
        assert_eq!(
            (inconsistent.workers, inconsistent.working_set_budget.0),
            (2, 256 * MIB)
        );
    }
}
