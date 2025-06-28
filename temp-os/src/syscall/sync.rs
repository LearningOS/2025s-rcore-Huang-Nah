use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
/// sleep syscall
pub fn sys_sleep(ms: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_sleep",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}
/// mutex create syscall
pub fn sys_mutex_create(blocking: bool) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    let mutex_id = if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        id
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.mutex_list.len() - 1
    };
    
    // 如果启用了死锁检测，设置 mutex 资源数量为 1
    if process_inner.deadlock_detection_enabled {
        drop(process_inner);
        use crate::syscall::DEADLOCK_DETECTOR;
        let mut detector = DEADLOCK_DETECTOR.exclusive_access();
        detector.set_resource_count(mutex_id, 1);
        drop(detector);
    } else {
        drop(process_inner);
    }
    
    mutex_id as isize
}
/// mutex lock syscall
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_lock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let pid = process.getpid();
    
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    let deadlock_enabled = process_inner.deadlock_detection_enabled;
    drop(process_inner);
    drop(process);
    
    // 检查是否启用死锁检测
    if deadlock_enabled {
        use crate::syscall::DEADLOCK_DETECTOR;
        let mut detector = DEADLOCK_DETECTOR.exclusive_access();
        
        // 确保进程已初始化
        if !detector.allocation.contains_key(&pid) {
            detector.init_process(pid, 100); // 假设100种资源
        }
        
        // 对于 mutex，只检查重入，不检查资源可用性
        // 因为 mutex.lock() 本身会处理等待逻辑
        if let Some(alloc) = detector.allocation.get(&pid) {
            if mutex_id < alloc.len() && alloc[mutex_id] > 0 {
                // 进程已经持有该mutex，再次请求构成重入死锁
                warn!("Deadlock detected (reentrant): pid={}, mutex_id={}", pid, mutex_id);
                drop(detector);
                return -0xDEAD;
            }
        }
        drop(detector);
    }
    
    // 尝试获取锁
    mutex.lock();
    
    // 只有成功获取锁后，才更新死锁检测器状态
    if deadlock_enabled {
        use crate::syscall::DEADLOCK_DETECTOR;
        let mut detector = DEADLOCK_DETECTOR.exclusive_access();
        // 更新分配状态
        detector.allocate_resource(pid, mutex_id);
        drop(detector);
    }
    
    0
}
/// mutex unlock syscall
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_unlock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let pid = process.getpid();
    
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    let deadlock_enabled = process_inner.deadlock_detection_enabled;
    drop(process_inner);
    drop(process);
    
    // 先解锁 mutex
    mutex.unlock();
    
    // 然后更新死锁检测器状态
    if deadlock_enabled {
        use crate::syscall::DEADLOCK_DETECTOR;
        let mut detector = DEADLOCK_DETECTOR.exclusive_access();
        detector.release_resource(pid, mutex_id);
        drop(detector);
    }
    0
}
/// semaphore create syscall
pub fn sys_semaphore_create(res_count: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem_id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.semaphore_list.len() - 1
    };
    
    // 如果启用了死锁检测，设置 semaphore 资源数量
    if process_inner.deadlock_detection_enabled {
        drop(process_inner);
        use crate::syscall::DEADLOCK_DETECTOR;
        let mut detector = DEADLOCK_DETECTOR.exclusive_access();
        // semaphore资源ID = mutex数量 + sem_id
        let resource_id = 50 + sem_id;
        detector.set_resource_count(resource_id, res_count);
        drop(detector);
    } else {
        drop(process_inner);
    }
    
    sem_id as isize
}
/// semaphore up syscall
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_up",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let pid = process.getpid();
    
    let process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    let deadlock_enabled = process_inner.deadlock_detection_enabled;
    drop(process_inner);
    drop(process);
    
    // 先调用 semaphore up
    sem.up();
    
    // 如果启用了死锁检测，更新状态
    if deadlock_enabled {
        use crate::syscall::DEADLOCK_DETECTOR;
        let mut detector = DEADLOCK_DETECTOR.exclusive_access();
        let resource_id = 50 + sem_id;
        detector.release_resource(pid, resource_id);
        drop(detector);
    }
    
    0
}
/// semaphore down syscall
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_down",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let pid = process.getpid();
    
    // 检查是否启用死锁检测
    {
        let process_inner = process.inner_exclusive_access();
        if process_inner.deadlock_detection_enabled {
            drop(process_inner);
            
            use crate::syscall::DEADLOCK_DETECTOR;
            let mut detector = DEADLOCK_DETECTOR.exclusive_access();
            
            // 确保进程已初始化
            if !detector.allocation.contains_key(&pid) {
                detector.init_process(pid, 100); // 假设100种资源
            }
            
            // semaphore资源ID = mutex数量 + sem_id  
            let resource_id = 50 + sem_id;
            
            // 使用银行家算法进行安全性检查
            if !detector.is_safe_if_allocate(pid, resource_id) {
                warn!("Deadlock detected (semaphore): pid={}, sem_id={}", pid, sem_id);
                drop(detector);
                return -0xDEAD; // 检测到死锁
            }
            drop(detector);
        } else {
            drop(process_inner);
        }
    }
    
    let process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    let deadlock_enabled = process_inner.deadlock_detection_enabled;
    drop(process_inner);
    
    // 获取 semaphore
    sem.down();
    
    // 如果启用了死锁检测，成功获取后更新状态
    if deadlock_enabled {
        use crate::syscall::DEADLOCK_DETECTOR;
        let mut detector = DEADLOCK_DETECTOR.exclusive_access();
        let resource_id = 50 + sem_id;
        detector.allocate_resource(process.getpid(), resource_id);
        drop(detector);
    }
    
    0
}
/// condvar create syscall
pub fn sys_condvar_create() -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}
/// condvar signal syscall
pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_signal",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}
/// condvar wait syscall
pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_wait",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    
    // 简化：直接调用 condvar.wait，内部会处理 mutex 的 unlock 和 lock
    // 不需要手动处理死锁检测器状态，因为 wait 方法会调用 unlock/lock
    // 而这些方法已经有死锁检测器的逻辑
    condvar.wait(mutex);
    
    0
}
/// enable deadlock detection syscall
///
/// YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(_enabled: usize) -> isize {
    // trace!("kernel: sys_enable_deadlock_detect NOT IMPLEMENTED");
    // -1
    let process = current_process();
    let mut inner = process.inner_exclusive_access();

    match _enabled {
        1 => inner.deadlock_detection_enabled = true,
        0=> inner.deadlock_detection_enabled = false,
        _=> return -1,
    }
    0

}
