//! Diagnostic-only per-stage accounting for the user page-fault path.
//!
//! The default (non-THP) fault path is two orders of magnitude slower than an
//! explicit huge-page fault on SG2002; attributing that gap needs the cost of
//! each stage (plan / prepare / apply / TLB flush / MMU cache update) rather
//! than one aggregate number.  Counters are cumulative; read
//! `/proc/fault_attrib` before and after a workload and subtract.
//!
//! Costs a few `rdtime` reads per fault, so this is a bring-up diagnostic, not
//! something to keep in a shipped build.

use alloc::string::String;
use alloc::format;
use core::sync::atomic::{AtomicU64, Ordering};

pub const STAGE_PLAN: usize = 0;
pub const STAGE_PREPARE: usize = 1;
pub const STAGE_APPLY: usize = 2;
pub const STAGE_TLB: usize = 3;
pub const STAGE_MMU_CACHE: usize = 4;
pub const STAGE_FRAME_ALLOC: usize = 5;
pub const STAGE_FRAME_ZERO: usize = 6;
pub const STAGE_MATERIALIZE: usize = 7;
pub const STAGE_DEPOSIT_PREP: usize = 8;
pub const STAGE_PAGE_OBJECT: usize = 9;
pub const STAGE_PENDING_INSERT: usize = 10;
pub const STAGE_APPLY_PREP: usize = 11;
pub const STAGE_APPLY_MAP: usize = 12;
pub const STAGE_APPLY_PUBLISH: usize = 13;
pub const STAGE_FAULT_AROUND: usize = 14;
/// 顺序缺页预取窗口内部的阶段（`STAGE_FA_APPLY` 是 `FA_BACKEND`/`FA_PUBLISH`
/// 的超集，和 `materialize` 与 `page_object` 的关系一样）。
pub const STAGE_FA_CHECK: usize = 15;
pub const STAGE_FA_PREIMAGE: usize = 16;
pub const STAGE_FA_APPLY: usize = 17;
pub const STAGE_FA_BACKEND: usize = 18;
pub const STAGE_FA_PUBLISH: usize = 19;
pub const STAGE_FA_COMMIT: usize = 20;
/// COW 页索引插入内部的细分（诊断 `pending_insert` 为什么随区域增长）。
pub const STAGE_PEND_LOCK: usize = 21;
pub const STAGE_PEND_SEARCH: usize = 22;
pub const STAGE_PEND_OVERLAP: usize = 23;
pub const STAGE_PEND_STORE: usize = 24;
/// `PageObject` 创建的两半：帧租约 vs 对象本体。
pub const STAGE_PO_LEASE: usize = 25;
pub const STAGE_PO_NEW: usize = 26;
/// H5b：把 open / read / close 三个系统调用分开计时（定位 ext4 open 路径的 ~10 ms）。
pub const STAGE_SYSCALL_OPEN: usize = 27;
pub const STAGE_SYSCALL_READ: usize = 28;
pub const STAGE_SYSCALL_CLOSE: usize = 29;
/// H5b：close 内部再拆五段（表摘除 / on_close / 锁清理 / 文件析构 / 唤醒）。
pub const STAGE_CLOSE_TABLE: usize = 30;
pub const STAGE_CLOSE_ONCLOSE: usize = 31;
pub const STAGE_CLOSE_LOCKS: usize = 32;
pub const STAGE_CLOSE_DROP: usize = 33;
pub const STAGE_CLOSE_WAKE: usize = 34;
/// H5d：把 `drop(fd)` 拆成"只减引用"与"真正析构文件对象"两段。
pub const STAGE_CLOSE_DROP_INNER: usize = 35;
/// H1b：execve 分段（定位 36 ms 的 exec 成本）。
pub const STAGE_EXEC_TOTAL: usize = 36;
pub const STAGE_EXEC_LOAD: usize = 37;
pub const STAGE_EXEC_ELF: usize = 38;
pub const STAGE_EXEC_STACK: usize = 39;
pub const STAGE_EXEC_INSTALL: usize = 40;
/// H1c：`exec_elf` 内部的细分（解析 / 段映射 / PIE populate / 重定位）。
pub const STAGE_EXEC_PARSE: usize = 41;
pub const STAGE_EXEC_MAP: usize = 42;
pub const STAGE_EXEC_POPULATE: usize = 43;
pub const STAGE_EXEC_RELOC: usize = 44;
/// H1d：file-backed `populate` 循环内部按页细分（定位 120 µs/页花在哪）。
pub const STAGE_FILE_POP_QUERY: usize = 45;
pub const STAGE_FILE_POP_PIN: usize = 46;
pub const STAGE_FILE_POP_POBJ: usize = 47;
pub const STAGE_FILE_POP_PREP: usize = 48;
pub const STAGE_FILE_POP_MAP: usize = 49;
/// C1：fork 分段（定位 7.4 ms 的 fork 成本落在"页表复制 / 父侧写保护"哪一段）。
///
/// `fork_exit` 探针是 fork()+_exit()+waitpid() 的整体耗时，而 fork 本身要扫父
/// 页表两遍（`prepare_fork_parent_mutation` + 每个 VMA 的 `clone_map`），父侧写
/// 保护还要再扫一遍（`apply_fork_parent_mutation`）。这组计数器把这几遍分开，
/// 并顺带量出子地址空间析构（占用 `_exit()` 的那半边）。
pub const STAGE_FORK_TOTAL: usize = 50;
pub const STAGE_FORK_PARENT_PREP: usize = 51;
pub const STAGE_FORK_CLONE_MAP: usize = 52;
pub const STAGE_FORK_CLONE_WALK: usize = 53;
pub const STAGE_FORK_CLONE_ENTRY: usize = 54;
pub const STAGE_FORK_APPLY_PARENT: usize = 55;
pub const STAGE_FORK_PREP_PROC: usize = 56;
pub const STAGE_EXIT_ASPACE: usize = 57;
/// C1 细分：`clone_map` 的每叶循环 vs 子侧 `map_page`，以及子地址空间析构的两半。
pub const STAGE_FORK_CLONE_LEAF: usize = 58;
pub const STAGE_FORK_CLONE_PTE: usize = 59;
pub const STAGE_FORK_PUB_PREP: usize = 60;
pub const STAGE_FORK_PUB_APPLY: usize = 61;
pub const STAGE_EXIT_CLEAR: usize = 62;
pub const STAGE_EXIT_DETACH: usize = 63;
/// C1 粗粒度：把 `fork()` / `_exit()` / `waitpid()` 三个系统调用各自计时，
/// 好把 `fork_exit` 探针的 7.5 ms 先劈成两半再细看。全部按"每次 fork"取平均
/// （`exit`/`wait` 的次数与 fork 同量级）。
pub const STAGE_FORK_SYSCALL: usize = 64;
pub const STAGE_EXIT_SYSCALL: usize = 65;
pub const STAGE_WAIT_SYSCALL: usize = 66;
/// C1：`do_clone` 里"地址空间之外"的几大块（fork 系统调用总耗时的大头）。
pub const STAGE_FORK_PID: usize = 67;
pub const STAGE_FORK_CGROUP: usize = 68;
pub const STAGE_FORK_NSPROXY: usize = 69;
pub const STAGE_FORK_IMAGE: usize = 70;
pub const STAGE_FORK_SCOPE: usize = 71;
pub const STAGE_FORK_TASK: usize = 72;
/// C1：`fork_task`（2.5 ms）与 `do_exit`（1.1 ms）内部的再细分。
pub const STAGE_FORK_PREP_THREAD: usize = 73;
pub const STAGE_FORK_STAGE_PUB: usize = 74;
pub const STAGE_FORK_ACTIVATE: usize = 75;
pub const STAGE_EXIT_FD: usize = 76;
pub const STAGE_EXIT_MM: usize = 77;
pub const STAGE_EXIT_PROC: usize = 78;
pub const STAGE_EXIT_PUB: usize = 79;
/// C1：`fork_clone_entry`（每个 VMA ~700 µs）内部再拆 memfd 增量与 VMA 树操作。
pub const STAGE_FORK_MEMFD: usize = 80;
pub const STAGE_FORK_VMA: usize = 81;
/// B1/B2：用户陷阱循环里"一次系统调用"的入口/分派/出口分段。`syscost2` 已经
/// 证明 `invalid_syscall` 8.5 µs ≈ 空系统调用地板、`getpid` 9.8 µs ⇒ 成本几乎
/// 全在这套机制上，而不在 handler 里。这组计数器把它拆开定位。
pub const STAGE_SYS_ITER: usize = 82;
pub const STAGE_SYS_UCTX_ENTER: usize = 83;
pub const STAGE_SYS_PTRACE_PRE: usize = 84;
pub const STAGE_SYS_HANDLE: usize = 85;
pub const STAGE_SYS_POST: usize = 86;
/// B1 细分：每次返回用户态都要做的 `fence.i` 与裸的用户往返。
pub const STAGE_SYS_FENCE_I: usize = 87;
pub const STAGE_SYS_USER_ROUNDTRIP: usize = 88;
pub const STAGE_SYS_PREP_RETURN: usize = 89;
const STAGES: usize = 90;

static TOTALS: [AtomicU64; STAGES] = [const { AtomicU64::new(0) }; STAGES];
static FAULTS: AtomicU64 = AtomicU64::new(0);
/// fork(2) 的调用次数 —— fork 各分段按"每次 fork"取平均，而不是按缺页数。
static FORKS: AtomicU64 = AtomicU64::new(0);
/// 进程退出时销毁地址空间的次数（`fork_exit` 探针的另一半）。
static EXITS: AtomicU64 = AtomicU64::new(0);
/// `do_exit` 的调用次数（每个退出的线程一次），用于 `exit_fd/mm/proc/pub` 取平均。
static EXIT_CALLS: AtomicU64 = AtomicU64::new(0);
/// 用户陷阱循环的迭代次数（≈ 系统调用 + 缺页 + 中断的次数）。
static TRAP_ITERS: AtomicU64 = AtomicU64::new(0);

/// 有序索引插入的位置直方图（诊断 COW 页索引的搬移代价）。
///
/// 分桶：0 = 插在最前（要搬走整个数组）、1 = 追加在尾部（不用搬）、
/// 2..5 = 距离尾部 16/64/256/更多 个条目。`SKIPPED_ENTRIES` 是累计被搬移
/// 的条目数，乘以条目大小就是这段真正搬了多少字节。
static INSERT_POSITIONS: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];
static INSERT_SHIFTED_ENTRIES: AtomicU64 = AtomicU64::new(0);

/// 匿名批量缺页的整块分配结果：成功了多少页、回退逐页了多少页、试了几次。
static ANON_BLOCK_PAGES_OK: AtomicU64 = AtomicU64::new(0);
static ANON_BLOCK_FALLBACK_PAGES: AtomicU64 = AtomicU64::new(0);
static ANON_BLOCK_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// Records one contiguous-block attempt's outcome.
pub fn note_anon_block(pages: u64) {
    ANON_BLOCK_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    ANON_BLOCK_PAGES_OK.fetch_add(pages, Ordering::Relaxed);
}

/// Records `pages` filled by the per-page fallback.
pub fn note_anon_block_fallback(pages: u64) {
    ANON_BLOCK_FALLBACK_PAGES.fetch_add(pages, Ordering::Relaxed);
}

/// Records one ordered-index insertion's shift distance.
pub fn note_insert_position(position: usize, len: usize) {
    let shifted = len.saturating_sub(position);
    let bucket = if position == 0 {
        0
    } else if position == len {
        1
    } else if shifted <= 16 {
        2
    } else if shifted <= 64 {
        3
    } else if shifted <= 256 {
        4
    } else {
        5
    };
    INSERT_POSITIONS[bucket].fetch_add(1, Ordering::Relaxed);
    INSERT_SHIFTED_ENTRIES.fetch_add(shifted as u64, Ordering::Relaxed);
}

/// Reads the monotonic clock used for the deltas.
pub fn stage_now() -> u64 {
    ax_hal::time::monotonic_time_nanos()
}

/// Adds one stage's elapsed nanoseconds.
pub fn add(stage: usize, ns: u64) {
    TOTALS[stage].fetch_add(ns, Ordering::Relaxed);
}

/// Timestamps the enclosing scope and attributes it to `stage` on drop.
///
/// H5b 的用法：在系统调用入口挂一个 `let _t = fault_attrib::scope(STAGE_X);`，
/// 就能把 open / read / close 各自的总耗时拆出来（`_avg` 仍是"每次系统调用"）。
#[must_use]
pub struct Scope {
    stage: usize,
    start: u64,
}

impl Scope {
    pub fn new(stage: usize) -> Self {
        Self {
            stage,
            start: stage_now(),
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let end = stage_now();
        if end > self.start {
            add(self.stage, end - self.start);
        }
    }
}

/// Convenience constructor for [`Scope`].
pub fn scope(stage: usize) -> Scope {
    Scope::new(stage)
}

/// 采样上限：超过这个时长的样本一律丢弃。
///
/// 系统调用入口/出口这类"每线程都在走"的路径上，**阻塞型系统调用**（例如
/// 空闲 shell 卡在 `read()` 上等命令结束）会把整段墙钟算进去，一个样本就能
/// 把均值抬高几个数量级。按 Linux 的 sched_switch 计时口径，这种样本本来就
/// 不该算进"处理一次系统调用要多久"，所以直接丢。
pub const SAMPLE_CAP_NS: u64 = 100_000;

/// [`Scope`] 的"丢弃阻塞样本"版本，供每线程热路径使用。
#[must_use]
pub struct ScopeCapped {
    stage: usize,
    start: u64,
}

impl ScopeCapped {
    pub fn new(stage: usize) -> Self {
        Self {
            stage,
            start: stage_now(),
        }
    }
}

impl Drop for ScopeCapped {
    fn drop(&mut self) {
        let end = stage_now();
        if end > self.start {
            let delta = end - self.start;
            if delta < SAMPLE_CAP_NS {
                add(self.stage, delta);
            }
        }
    }
}

/// Convenience constructor for [`ScopeCapped`].
pub fn scope_capped(stage: usize) -> ScopeCapped {
    ScopeCapped::new(stage)
}

/// Counts one resolved (or attempted) fault.
pub fn note_fault() {
    FAULTS.fetch_add(1, Ordering::Relaxed);
}

/// Counts `pages` resolved pages at once.  The sequential fault-around path
/// publishes a whole window from one trap, so counting pages (not traps) keeps
/// every `<stage>_avg` in the table a per-page figure.
pub fn note_faults(pages: u64) {
    FAULTS.fetch_add(pages, Ordering::Relaxed);
}

/// Counts one `fork(2)` (i.e. one non-`CLONE_VM` clone).
pub fn note_fork() {
    FORKS.fetch_add(1, Ordering::Relaxed);
}

/// Fork 深度：`publish_prepared_pte_owners` 是缺页与 fork 共用的路径，只有
/// 嵌套在 `AddrSpace::try_clone` 里的那次才该记到 `fork_pub_*` 名下。
static FORK_DEPTH: AtomicU64 = AtomicU64::new(0);

/// Marks the enclosing scope as running inside `AddrSpace::try_clone`.
#[must_use]
pub struct ForkDepthGuard;

impl ForkDepthGuard {
    pub fn new() -> Self {
        FORK_DEPTH.fetch_add(1, Ordering::Relaxed);
        Self
    }
}

impl Drop for ForkDepthGuard {
    fn drop(&mut self) {
        FORK_DEPTH.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Whether the current thread is inside `AddrSpace::try_clone`.
pub fn in_fork() -> bool {
    FORK_DEPTH.load(Ordering::Relaxed) != 0
}

/// Counts one address-space teardown performed by process exit.
pub fn note_exit_aspace() {
    EXITS.fetch_add(1, Ordering::Relaxed);
}

/// Counts one `do_exit` call.
pub fn note_exit_call() {
    EXIT_CALLS.fetch_add(1, Ordering::Relaxed);
}

/// Counts one iteration of the user trap loop.
pub fn note_trap_iter() {
    TRAP_ITERS.fetch_add(1, Ordering::Relaxed);
}

/// Renders the cumulative per-stage table (for `/proc/fault_attrib`).
pub fn render() -> String {
    let faults = FAULTS.load(Ordering::Relaxed);
    let mut total = 0u64;
    let mut out = String::new();
    for (name, stage) in [
        ("plan", STAGE_PLAN),
        ("prepare", STAGE_PREPARE),
        ("apply", STAGE_APPLY),
        ("tlb", STAGE_TLB),
        ("mmu_cache", STAGE_MMU_CACHE),
        ("frame_alloc", STAGE_FRAME_ALLOC),
        ("frame_zero", STAGE_FRAME_ZERO),
        ("materialize", STAGE_MATERIALIZE),
        ("deposit_prep", STAGE_DEPOSIT_PREP),
        ("page_object", STAGE_PAGE_OBJECT),
        ("pending_insert", STAGE_PENDING_INSERT),
        ("apply_prep", STAGE_APPLY_PREP),
        ("apply_map", STAGE_APPLY_MAP),
        ("apply_publish", STAGE_APPLY_PUBLISH),
        ("fault_around", STAGE_FAULT_AROUND),
        ("fa_check", STAGE_FA_CHECK),
        ("fa_preimage", STAGE_FA_PREIMAGE),
        ("fa_apply", STAGE_FA_APPLY),
        ("fa_backend", STAGE_FA_BACKEND),
        ("fa_publish", STAGE_FA_PUBLISH),
        ("fa_commit", STAGE_FA_COMMIT),
        ("pend_lock", STAGE_PEND_LOCK),
        ("pend_search", STAGE_PEND_SEARCH),
        ("pend_overlap", STAGE_PEND_OVERLAP),
        ("pend_store", STAGE_PEND_STORE),
        ("po_lease", STAGE_PO_LEASE),
        ("po_new", STAGE_PO_NEW),
        ("syscall_open", STAGE_SYSCALL_OPEN),
        ("syscall_read", STAGE_SYSCALL_READ),
        ("syscall_close", STAGE_SYSCALL_CLOSE),
        ("close_table", STAGE_CLOSE_TABLE),
        ("close_onclose", STAGE_CLOSE_ONCLOSE),
        ("close_locks", STAGE_CLOSE_LOCKS),
        ("close_drop", STAGE_CLOSE_DROP),
        ("close_wake", STAGE_CLOSE_WAKE),
        ("close_drop_inner", STAGE_CLOSE_DROP_INNER),
        ("exec_total", STAGE_EXEC_TOTAL),
        ("exec_load", STAGE_EXEC_LOAD),
        ("exec_elf", STAGE_EXEC_ELF),
        ("exec_stack", STAGE_EXEC_STACK),
        ("exec_install", STAGE_EXEC_INSTALL),
        ("exec_parse", STAGE_EXEC_PARSE),
        ("exec_map", STAGE_EXEC_MAP),
        ("exec_populate", STAGE_EXEC_POPULATE),
        ("exec_reloc", STAGE_EXEC_RELOC),
        ("file_pop_query", STAGE_FILE_POP_QUERY),
        ("file_pop_pin", STAGE_FILE_POP_PIN),
        ("file_pop_pobj", STAGE_FILE_POP_POBJ),
        ("file_pop_prep", STAGE_FILE_POP_PREP),
        ("file_pop_map", STAGE_FILE_POP_MAP),
    ] {
        let ns = TOTALS[stage].load(Ordering::Relaxed);
        total += ns;
        let avg = if faults == 0 { 0 } else { ns / faults };
        out.push_str(&format!("{name}_ns={ns} {name}_avg={avg}\n"));
    }
    out.push_str(&format!(
        "total_ns={total} total_avg={}\nfaults={faults}\n",
        if faults == 0 { 0 } else { total / faults }
    ));
    {
        // fork 分段：按"每次 fork"取平均。`fork_total` 只覆盖地址空间克隆
        // （`AddrSpace::try_clone`），不含任务创建与调度。
        let forks = FORKS.load(Ordering::Relaxed).max(1);
        let mut fork_total = 0u64;
        for (name, stage) in [
            ("fork_total", STAGE_FORK_TOTAL),
            ("fork_parent_prep", STAGE_FORK_PARENT_PREP),
            ("fork_clone_map", STAGE_FORK_CLONE_MAP),
            ("fork_clone_walk", STAGE_FORK_CLONE_WALK),
            ("fork_clone_entry", STAGE_FORK_CLONE_ENTRY),
            ("fork_apply_parent", STAGE_FORK_APPLY_PARENT),
            ("fork_prep_proc", STAGE_FORK_PREP_PROC),
            ("fork_clone_leaf", STAGE_FORK_CLONE_LEAF),
            ("fork_clone_pte", STAGE_FORK_CLONE_PTE),
            ("fork_pub_prep", STAGE_FORK_PUB_PREP),
            ("fork_pub_apply", STAGE_FORK_PUB_APPLY),
            ("fork_syscall", STAGE_FORK_SYSCALL),
            ("exit_syscall", STAGE_EXIT_SYSCALL),
            ("wait_syscall", STAGE_WAIT_SYSCALL),
            ("fork_pid", STAGE_FORK_PID),
            ("fork_cgroup", STAGE_FORK_CGROUP),
            ("fork_nsproxy", STAGE_FORK_NSPROXY),
            ("fork_image", STAGE_FORK_IMAGE),
            ("fork_scope", STAGE_FORK_SCOPE),
            ("fork_task", STAGE_FORK_TASK),
            ("fork_prep_thread", STAGE_FORK_PREP_THREAD),
            ("fork_stage_pub", STAGE_FORK_STAGE_PUB),
            ("fork_activate", STAGE_FORK_ACTIVATE),
            ("fork_memfd", STAGE_FORK_MEMFD),
            ("fork_vma", STAGE_FORK_VMA),
        ] {
            let ns = TOTALS[stage].load(Ordering::Relaxed);
            fork_total += ns;
            out.push_str(&format!("{name}_ns={ns} {name}_avg={}\n", ns / forks));
        }
        out.push_str(&format!(
            "fork_calls={} fork_accounted_avg={}\n",
            FORKS.load(Ordering::Relaxed),
            fork_total / forks
        ));
        let exits = EXITS.load(Ordering::Relaxed).max(1);
        for (name, stage) in [
            ("exit_aspace", STAGE_EXIT_ASPACE),
            ("exit_clear", STAGE_EXIT_CLEAR),
            ("exit_detach", STAGE_EXIT_DETACH),
        ] {
            let ns = TOTALS[stage].load(Ordering::Relaxed);
            out.push_str(&format!("{name}_ns={ns} {name}_avg={}\n", ns / exits));
        }
        out.push_str(&format!(
            "exit_aspace_calls={}\n",
            EXITS.load(Ordering::Relaxed)
        ));
        let exit_calls = EXIT_CALLS.load(Ordering::Relaxed).max(1);
        for (name, stage) in [
            ("exit_fd", STAGE_EXIT_FD),
            ("exit_mm", STAGE_EXIT_MM),
            ("exit_proc", STAGE_EXIT_PROC),
            ("exit_pub", STAGE_EXIT_PUB),
        ] {
            let ns = TOTALS[stage].load(Ordering::Relaxed);
            out.push_str(&format!("{name}_ns={ns} {name}_avg={}\n", ns / exit_calls));
        }
        out.push_str(&format!(
            "exit_calls={}\n",
            EXIT_CALLS.load(Ordering::Relaxed)
        ));
        let iters = TRAP_ITERS.load(Ordering::Relaxed).max(1);
        for (name, stage) in [
            ("sys_iter", STAGE_SYS_ITER),
            ("sys_uctx_enter", STAGE_SYS_UCTX_ENTER),
            ("sys_ptrace_pre", STAGE_SYS_PTRACE_PRE),
            ("sys_handle", STAGE_SYS_HANDLE),
            ("sys_post", STAGE_SYS_POST),
            ("sys_fence_i", STAGE_SYS_FENCE_I),
            ("sys_user_roundtrip", STAGE_SYS_USER_ROUNDTRIP),
            ("sys_prep_return", STAGE_SYS_PREP_RETURN),
        ] {
            let ns = TOTALS[stage].load(Ordering::Relaxed);
            out.push_str(&format!("{name}_ns={ns} {name}_avg={}\n", ns / iters));
        }
        out.push_str(&format!(
            "trap_iters={}\n",
            TRAP_ITERS.load(Ordering::Relaxed)
        ));
    }
    let (reclaim_calls, reclaim_ns) = ax_alloc::reclaim_stats();
    out.push_str(&format!(
        "reclaim_calls={reclaim_calls} reclaim_ns={reclaim_ns} reclaim_avg={}\n",
        if reclaim_calls == 0 { 0 } else { reclaim_ns / reclaim_calls }
    ));
    let positions: [u64; 6] = core::array::from_fn(|i| INSERT_POSITIONS[i].load(Ordering::Relaxed));
    let shifted = INSERT_SHIFTED_ENTRIES.load(Ordering::Relaxed);
    let inserts: u64 = positions.iter().sum();
    out.push_str(&format!(
        "insert_total={inserts} insert_front={} insert_append={} insert_near16={} \
         insert_near64={} insert_near256={} insert_far={} insert_shifted_entries={shifted} \
         insert_shifted_per_insert={}\n",
        positions[0], positions[1], positions[2], positions[3], positions[4], positions[5],
        if inserts == 0 { 0 } else { shifted / inserts }
    ));
    out.push_str(&format!(
        "anon_block_attempts={} anon_block_pages_ok={} anon_block_fallback_pages={} \
         anon_block_hit_ratio={}\n",
        ANON_BLOCK_ATTEMPTS.load(Ordering::Relaxed),
        ANON_BLOCK_PAGES_OK.load(Ordering::Relaxed),
        ANON_BLOCK_FALLBACK_PAGES.load(Ordering::Relaxed),
        {
            let ok = ANON_BLOCK_PAGES_OK.load(Ordering::Relaxed);
            let fallback = ANON_BLOCK_FALLBACK_PAGES.load(Ordering::Relaxed);
            if ok + fallback == 0 {
                0
            } else {
                ok * 100 / (ok + fallback)
            }
        }
    ));
    {
        // MM 生命周期队列：repair 一增长就代表"整片地址空间被挂住、内存只增不减"
        // （回收失败会被塞进 REPAIR_QUEUE，只有显式请求才重试）。
        let (retire, repair, retry_pending) = crate::mm::mm_queue_lengths();
        out.push_str(&format!(
            "mm_retire_queue={retire} mm_repair_queue={repair} \
             mm_repair_retry_pending={}\n",
            u8::from(retry_pending)
        ));
    }
    {
        // 页缓存身份复用情况（H5 诊断）：created 高 = 每次 open 都重建身份。
        let (by_location, by_inode, created) = ax_fs_ng::cached_file_identity_stats();
        out.push_str(&format!(
            "cache_identity_from_location={by_location} cache_identity_from_inode={by_inode} \
             cache_identity_created={created}\n"
        ));
    }
    // FS 侧分段（ax-fs-ng 自己的计数器，见 fs/ax-fs-ng/src/diag.rs）。
    out.push_str(&ax_fs_ng::diag::render());
    // ax-task 侧分段（见 components/ax-task/src/diag.rs）。
    out.push_str(&ax_std::os::arceos::task::diag::render());
    // ax-cpu 侧分段（见 components/axcpu/src/diag.rs）。
    out.push_str(&ax_cpu::diag::render());
    // C1：地址空间标签能力（1 = 只能全量刷 TLB，值越大 = 能用硬件 ASID）。
    out.push_str(&format!(
        "asid_tag_capacity={}\n",
        ax_runtime::hal::cache::address_space_tag_capacity()
    ));
    out
}
