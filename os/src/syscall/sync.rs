use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
use alloc::vec;
use crate::syscall::thread::sys_gettid;

pub fn sys_sleep(ms: usize) -> isize {
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}

// LAB5 HINT: you might need to maintain data structures used for deadlock detection
// during sys_mutex_* and sys_semaphore_* syscalls
pub fn sys_mutex_create(blocking: bool) -> isize {
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id) {
        process_inner.mutex_list[id] = mutex;
        // update mutex properties
        process_inner.mutex_available.insert(id, 1);
        for thread_mutex_allocation in &mut process_inner.mutex_allocation {
            thread_mutex_allocation.insert(id, 0);
        }
        for thread_mutex_need in &mut process_inner.mutex_need {
            thread_mutex_need.insert(id, 0);
        }
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        // update mutex properties
        process_inner.mutex_available.push(1);
        for thread_mutex_allocation in &mut process_inner.mutex_allocation {
            thread_mutex_allocation.push(0);
        }
        for thread_mutex_need in &mut process_inner.mutex_need {
            thread_mutex_need.push(0);
        }
        process_inner.mutex_list.len() as isize - 1
    }
}

// LAB5 HINT: Return -0xDEAD if deadlock is detected
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    let tid = sys_gettid() as usize;
    let tnum = process_inner.mutex_list.len();
    let mnum = process_inner.mutex_available.len();
    if process_inner.mutex_available[mutex_id] == 0 {
        process_inner.mutex_need[tid][mutex_id] += 1;
        // deadlock detection
        if process_inner.is_enable_deadlock_detect == true {
            let mut work = process_inner.mutex_available.clone();
            let mut finish = vec![false; tnum];
            let mut cnt = 0;
            while cnt != tnum {
                cnt = 0;
                for i in 0..tnum {
                    if finish[i] == false {
                        let mut is_executable = true;
                        for j in 0..mnum {
                            if  work[j] < process_inner.mutex_need[i][j] {
                                is_executable = false;
                            }
                        }
                        if is_executable == true {
                            for j in 0..mnum {
                                work[j] += process_inner.mutex_allocation[i][j];
                            }
                            finish[i] = true;
                            continue;
                        }
                    }
                    cnt += 1;
                }
            }
            for f in finish {
                if f == false {
                    process_inner.mutex_need[tid][mutex_id] -= 1;
                    return -0xdead;
                }
            }
        }
    } else {
        process_inner.mutex_available[mutex_id] -= 1;
        process_inner.mutex_allocation[tid][mutex_id] += 1;
    }
    drop(process_inner);
    drop(process);
    mutex.lock();
    0
}

pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    let tid = sys_gettid() as usize;
    process_inner.mutex_available[mutex_id] += 1;
    process_inner.mutex_allocation[tid][mutex_id] -= 1;
    let waking_tid = mutex.get_waking_tid();
    if waking_tid != -1 {
        process_inner.mutex_available[mutex_id] -= 1;
        process_inner.mutex_allocation[waking_tid as usize][mutex_id] += 1;
        process_inner.mutex_need[waking_tid as usize][mutex_id] -= 1;
    }
    drop(process_inner);
    drop(process);
    mutex.unlock();
    0
}

pub fn sys_semaphore_create(res_count: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id) {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        // update semaphore properties
        process_inner.semaphore_available.insert(id, res_count);
        for thread_semaphore_allocation in process_inner.semaphore_allocation.iter_mut() {
            thread_semaphore_allocation.insert(id, 0);
        }
        for thread_semaphore_need in process_inner.semaphore_need.iter_mut() {
            thread_semaphore_need.insert(id, 0);
        }
        id
    } else {
        process_inner.semaphore_list.push(Some(Arc::new(Semaphore::new(res_count))));
        // update semaphore properties
        process_inner.semaphore_available.push(res_count);
        for thread_semaphore_allocation in process_inner.semaphore_allocation.iter_mut() {
            thread_semaphore_allocation.push(0);
        }
        for thread_semaphore_need in process_inner.semaphore_need.iter_mut() {
            thread_semaphore_need.push(0);
        }
        process_inner.semaphore_list.len() - 1
    };
    id as isize
}

pub fn sys_semaphore_up(sem_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    let tid = sys_gettid() as usize;
    process_inner.semaphore_available[sem_id] += 1;
    process_inner.semaphore_allocation[tid][sem_id] -= 1;
    let waking_tid = sem.get_waking_tid();
    if waking_tid != -1 {
        process_inner.semaphore_available[sem_id] -= 1;
        process_inner.semaphore_allocation[waking_tid as usize][sem_id] += 1;
        process_inner.semaphore_need[waking_tid as usize][sem_id] -= 1;
    }
    drop(process_inner);
    sem.up();
    0
}

// LAB5 HINT: Return -0xDEAD if deadlock is detected
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    let tid = sys_gettid() as usize;
    let tnum = process_inner.semaphore_list.len();
    let snum = process_inner.semaphore_available.len();
    if process_inner.semaphore_available[sem_id] == 0 {
        process_inner.semaphore_need[tid][sem_id] += 1;
        // deadlock detection
        if process_inner.is_enable_deadlock_detect == true {
            let mut work = process_inner.semaphore_available.clone();
            let mut finish = vec![false; tnum];
            let mut cnt = 0;
            while cnt != tnum {
                cnt = 0;
                for i in 0..tnum {
                    if finish[i] == false {
                        let mut is_executable = true;
                        for j in 0..snum {
                            if  work[j] < process_inner.semaphore_need[i][j] {
                                is_executable = false;
                            }
                        }
                        if is_executable == true {
                            for j in 0..snum {
                                work[j] += process_inner.semaphore_allocation[i][j];
                            }
                            finish[i] = true;
                            continue;
                        }
                    }
                    cnt += 1;
                }
            }
            for f in finish {
                if f == false {
                    process_inner.semaphore_need[tid][sem_id] -= 1;
                    return -0xdead;
                }
            }
        }
    } else {
        process_inner.semaphore_available[sem_id] -= 1;
        process_inner.semaphore_allocation[tid][sem_id] += 1;
    }
    drop(process_inner);
    sem.down();
    0
}

pub fn sys_condvar_create(_arg: usize) -> isize {
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

pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}

pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    condvar.wait(mutex);
    0
}

// LAB5 YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(_enabled: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    return match _enabled {
        0 => {
            process_inner.is_enable_deadlock_detect = false;
            drop(process_inner);
            0
        }
        1 => {
            process_inner.is_enable_deadlock_detect = true;
            drop(process_inner);
            0
        }
        _ => {
            drop(process_inner);
            -1
        }
    }
}
