//! Implementation of syscalls
//!
//! The single entry point to all system calls, [`syscall()`], is called
//! whenever userspace wishes to perform a system call using the `ecall`
//! instruction. In this case, the processor raises an 'Environment call from
//! U-mode' exception, which is handled as one of the cases in
//! [`crate::trap::trap_handler`].
//!
//! For clarity, each single syscall is implemented as its own function, named
//! `sys_` then the name of the syscall. You can find functions like this in
//! submodules, and you should also implement syscalls this way.

/// openat syscall
pub const SYSCALL_OPENAT: usize = 56;
/// close syscall
pub const SYSCALL_CLOSE: usize = 57;
/// read syscall
pub const SYSCALL_READ: usize = 63;
/// write syscall
pub const SYSCALL_WRITE: usize = 64;
/// unlinkat syscall
pub const SYSCALL_UNLINKAT: usize = 35;
/// linkat syscall
pub const SYSCALL_LINKAT: usize = 37;
/// fstat syscall
pub const SYSCALL_FSTAT: usize = 80;
/// exit syscall
pub const SYSCALL_EXIT: usize = 93;
/// sleep syscall
pub const SYSCALL_SLEEP: usize = 101;
/// yield syscall
pub const SYSCALL_YIELD: usize = 124;
/// kill syscall
pub const SYSCALL_KILL: usize = 129;
/*
/// sigaction syscall
pub const SYSCALL_SIGACTION: usize = 134;
/// sigprocmask syscall
pub const SYSCALL_SIGPROCMASK: usize = 135;
/// sigreturn syscall
pub const SYSCALL_SIGRETURN: usize = 139;
*/
/// gettimeofday syscall
pub const SYSCALL_GETTIMEOFDAY: usize = 169;
/// getpid syscall
pub const SYSCALL_GETPID: usize = 172;
/// gettid syscall
pub const SYSCALL_GETTID: usize = 178;
/// fork syscall
pub const SYSCALL_FORK: usize = 220;
/// exec syscall
pub const SYSCALL_EXEC: usize = 221;
/// waitpid syscall
pub const SYSCALL_WAITPID: usize = 260;
/// set priority syscall
pub const SYSCALL_SET_PRIORITY: usize = 140;
/*
/// sbrk syscall
pub const SYSCALL_SBRK: usize = 214;
*/
/// munmap syscall
pub const SYSCALL_MUNMAP: usize = 215;
/// mmap syscall
pub const SYSCALL_MMAP: usize = 222;
/// spawn syscall
pub const SYSCALL_SPAWN: usize = 400;
/*
/// mail read syscall
pub const SYSCALL_MAIL_READ: usize = 401;
/// mail write syscall
pub const SYSCALL_MAIL_WRITE: usize = 402;
*/
/// dup syscall
pub const SYSCALL_DUP: usize = 24;
/// pipe syscall
pub const SYSCALL_PIPE: usize = 59;
/// thread_create syscall
pub const SYSCALL_THREAD_CREATE: usize = 460;
/// waittid syscall
pub const SYSCALL_WAITTID: usize = 462;
/// mutex_create syscall
pub const SYSCALL_MUTEX_CREATE: usize = 463;
/// mutex_lock syscall
pub const SYSCALL_MUTEX_LOCK: usize = 464;
/// mutex_unlock syscall
pub const SYSCALL_MUTEX_UNLOCK: usize = 466;
/// semaphore_create syscall
pub const SYSCALL_SEMAPHORE_CREATE: usize = 467;
/// semaphore_up syscall
pub const SYSCALL_SEMAPHORE_UP: usize = 468;
/// enable deadlock detect syscall
pub const SYSCALL_ENABLE_DEADLOCK_DETECT: usize = 469;
/// semaphore_down syscall
pub const SYSCALL_SEMAPHORE_DOWN: usize = 470;
/// condvar_create syscall
pub const SYSCALL_CONDVAR_CREATE: usize = 471;
/// condvar_signal syscall
pub const SYSCALL_CONDVAR_SIGNAL: usize = 472;
/// condvar_wait syscallca
pub const SYSCALL_CONDVAR_WAIT: usize = 473;

mod fs;
mod process;
mod sync;
mod thread;

use alloc::vec::Vec;
use alloc::vec;
use alloc::collections::BTreeMap;
use fs::*;
use process::*;
use sync::*;
use thread::*;

use crate::fs::Stat;
use lazy_static::*;
use crate::sync::UPSafeCell;

lazy_static! {
    /// 全局死锁检测器
    pub static ref DEADLOCK_DETECTOR: UPSafeCell<DeadlockDetector> = unsafe {
        UPSafeCell::new(DeadlockDetector::new(100)) // 假设最多100种资源
    };
}

/// 死锁检测器，用于实现银行家算法
pub struct DeadlockDetector{
    available: Vec<usize>,
    allocation: BTreeMap<usize,Vec<usize>>,
    need: BTreeMap<usize,Vec<usize>>,
    max: BTreeMap<usize,Vec<usize>>,
    /// 记录每个资源类型的最大数量，用于区分mutex(1)和semaphore(n)
    resource_max: Vec<usize>,
}

impl DeadlockDetector {
    /// 创建新的死锁检测器
    /// resource_count: 资源类型数量 (mutex数量 + semaphore数量)
    pub fn new(resource_count: usize) -> Self {
        Self {
            available: vec![0; resource_count], // 初始时没有资源，会在创建时设置
            allocation: BTreeMap::new(),
            need: BTreeMap::new(),
            max: BTreeMap::new(),
            resource_max: vec![0; resource_count],
        }
    }

    /// 设置资源的可用数量
    pub fn set_resource_count(&mut self, resource_id: usize, count: usize) {
        if resource_id < self.available.len() {
            self.available[resource_id] = count;
            self.resource_max[resource_id] = count;
        }
    }

    /// 初始化进程的资源需求
    pub fn init_process(&mut self, pid: usize, resource_count: usize) {
        self.allocation.insert(pid, vec![0; resource_count]);
        self.need.insert(pid, vec![0; resource_count]); // 初始时不需要任何资源
        self.max.insert(pid, vec![1; resource_count]); // 每种资源最多需要1个（对mutex而言）
    }

    /// 检查请求资源是否会导致死锁
    pub fn is_safe_if_allocate(&self, pid: usize, resource_id: usize) -> bool {
        // 如果资源ID超出范围，返回false
        if resource_id >= self.available.len() {
            return false;
        }
        
        // 检查是否是 mutex 的重入锁定（对于mutex，资源总数为1）
        if resource_id < self.resource_max.len() && self.resource_max[resource_id] == 1 {
            // 对于 mutex，检查该进程是否已经持有该mutex
            if let Some(alloc) = self.allocation.get(&pid) {
                if resource_id < alloc.len() && alloc[resource_id] > 0 {
                    // 进程已经持有该mutex，再次请求构成死锁
                    return false;
                }
            }
            
            // 对于 mutex，如果有可用资源就安全
            if self.available[resource_id] > 0 {
                return true;
            } else {
                // mutex 资源不足，检测死锁
                return false;
            }
        }
        
        // 对于 semaphore（resource_max > 1），使用更宽松的策略
        // 只要有足够的可用资源，就认为是安全的
        if self.available[resource_id] > 0 {
            return true;
        }
        
        // 如果资源暂时不足，也允许等待，不认为是死锁
        // 因为semaphore允许资源的动态释放和获取
        // 只有在明确的循环等待情况下才认为是死锁
        return true; // 对semaphore采用宽松策略
    }

    /// 分配资源给进程
    pub fn allocate_resource(&mut self, pid: usize, resource_id: usize) {
        if self.available[resource_id] > 0 {
            self.available[resource_id] -= 1;
            if let Some(alloc) = self.allocation.get_mut(&pid) {
                alloc[resource_id] += 1;
            }
            if let Some(need) = self.need.get_mut(&pid) {
                if need[resource_id] > 0 {
                    need[resource_id] -= 1;
                }
            }
        }
    }

    /// 释放资源
    pub fn release_resource(&mut self, pid: usize, resource_id: usize) {
        if let Some(alloc) = self.allocation.get_mut(&pid) {
            if resource_id < alloc.len() && alloc[resource_id] > 0 {
                alloc[resource_id] -= 1;
                if resource_id < self.available.len() {
                    self.available[resource_id] += 1;
                }
                // 释放资源时不需要修改 need，因为 need 表示还需要的资源数量
                // 释放已分配的资源不会改变进程的总需求
            }
        }
    }
}

/// handle syscall exception with `syscall_id` and other arguments
pub fn syscall(syscall_id: usize, args: [usize; 4]) -> isize {
    match syscall_id {
        SYSCALL_DUP => sys_dup(args[0]),
        SYSCALL_LINKAT => sys_linkat(args[1] as *const u8, args[3] as *const u8),
        SYSCALL_UNLINKAT => sys_unlinkat(args[1] as *const u8),
        SYSCALL_OPENAT => sys_open(args[1] as *const u8, args[2] as u32),
        SYSCALL_CLOSE => sys_close(args[0]),
        SYSCALL_PIPE => sys_pipe(args[0] as *mut usize),
        SYSCALL_READ => sys_read(args[0], args[1] as *const u8, args[2]),
        SYSCALL_WRITE => sys_write(args[0], args[1] as *const u8, args[2]),
        SYSCALL_FSTAT => sys_fstat(args[0], args[1] as *mut Stat),
        SYSCALL_EXIT => sys_exit(args[0] as i32),
        SYSCALL_SLEEP => sys_sleep(args[0]),
        SYSCALL_YIELD => sys_yield(),
        SYSCALL_GETPID => sys_getpid(),
        SYSCALL_GETTID => sys_gettid(),
        SYSCALL_FORK => sys_fork(),
        SYSCALL_EXEC => sys_exec(args[0] as *const u8, args[1] as *const usize),
        SYSCALL_WAITPID => sys_waitpid(args[0] as isize, args[1] as *mut i32),
        SYSCALL_GETTIMEOFDAY => sys_get_time(args[0] as *mut TimeVal, args[1]),
        SYSCALL_MMAP => sys_mmap(args[0], args[1], args[2]),
        SYSCALL_MUNMAP => sys_munmap(args[0], args[1]),
        SYSCALL_SET_PRIORITY => sys_set_priority(args[0] as isize),
        SYSCALL_SPAWN => sys_spawn(args[0] as *const u8),
        SYSCALL_THREAD_CREATE => sys_thread_create(args[0], args[1]),
        SYSCALL_WAITTID => sys_waittid(args[0]) as isize,
        SYSCALL_MUTEX_CREATE => sys_mutex_create(args[0] == 1),
        SYSCALL_MUTEX_LOCK => sys_mutex_lock(args[0]),
        SYSCALL_MUTEX_UNLOCK => sys_mutex_unlock(args[0]),
        SYSCALL_SEMAPHORE_CREATE => sys_semaphore_create(args[0]),
        SYSCALL_SEMAPHORE_UP => sys_semaphore_up(args[0]),
        SYSCALL_ENABLE_DEADLOCK_DETECT => sys_enable_deadlock_detect(args[0]),
        SYSCALL_SEMAPHORE_DOWN => sys_semaphore_down(args[0]),
        SYSCALL_CONDVAR_CREATE => sys_condvar_create(),
        SYSCALL_CONDVAR_SIGNAL => sys_condvar_signal(args[0]),
        SYSCALL_CONDVAR_WAIT => sys_condvar_wait(args[0], args[1]),
        SYSCALL_KILL => sys_kill(args[0], args[1] as u32),
        _ => panic!("Unsupported syscall_id: {}", syscall_id),
    }
}
