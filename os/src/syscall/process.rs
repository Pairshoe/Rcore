//! Process management syscalls
use crate::mm::{translated_refmut, translated_ref, translated_str, translated_byte_buffer, VirtAddr, MapPermission};
use crate::task::{add_task, current_begin_time, current_syscall_times, current_task, current_user_token,
                  exit_current_and_run_next, insert_current_memory_set, remove_current_memory_set, set_current_priority,
                  suspend_current_and_run_next, TaskStatus};
use crate::fs::{open_file, OpenFlags};
use crate::timer::get_time_us;
use alloc::sync::Arc;
use alloc::vec::Vec;
use crate::config::MAX_SYSCALL_NUM;
use alloc::string::String;
use core::mem;
use core::mem::size_of;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

#[derive(Clone, Copy)]
pub struct TaskInfo {
    pub status: TaskStatus,
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    pub time: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    debug!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    current_task().unwrap().pid.0 as isize
}

/// Syscall Fork which returns 0 for child process and child_pid for parent process
pub fn sys_fork() -> isize {
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

/// Syscall Exec which accepts the elf path
pub fn sys_exec(path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}


/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let task = current_task().unwrap();
    // find a child process

    // ---- access current TCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid()) {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB lock exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after removing from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child TCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB lock automatically
}

// YOUR JOB: 引入虚地址后重写 sys_get_time
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    let _us = get_time_us();
    let dsts = translated_byte_buffer(current_user_token(), _ts as *mut u8, size_of::<TimeVal>());
    unsafe {
        let src = mem::transmute::<TimeVal, [u8; 16]>(TimeVal {
            sec: _us / 1_000_000,
            usec: _us % 1_000_000,
        });
        for dst in dsts {
            dst.copy_from_slice(&src);
        }
    }
    0
}

// YOUR JOB: 引入虚地址后重写 sys_task_info
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    let _us = get_time_us();
    let _now = ((_us / 1_000_000) & 0xffff) * 1000 + ((_us % 1_000_000) / 1000);
    let dsts = translated_byte_buffer(current_user_token(), _ti as *mut u8, size_of::<TaskInfo>());
    unsafe {
        let src = mem::transmute::<TaskInfo, [u8; 2016]>(TaskInfo {
            status: TaskStatus::Running,
            syscall_times: current_syscall_times(),
            time: _now - current_begin_time(),
        });
        for dst in dsts {
            dst.copy_from_slice(&src);
        }
    }
    0
}

// YOUR JOB: 实现sys_set_priority，为任务添加优先级
pub fn sys_set_priority(_prio: isize) -> isize {
    if 2 <= _prio {
        set_current_priority(_prio as usize);
        return _prio;
    }
    -1
}

// YOUR JOB: 扩展内核以实现 sys_mmap 和 sys_munmap
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    let va = VirtAddr::from(_start);
    if va.page_offset() == 0 && _port & !0x7 == 0 && _port & 0x7 != 0 {
        let permission = MapPermission::from_bits((_port << 1 | 1 << 4) as u8).unwrap();
        if insert_current_memory_set(_start.into(), (_start + _len).into(), permission) == 0 {
            return 0;
        }
    }
    -1
}

pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    remove_current_memory_set(_start.into(), (_start + _len).into())
}

// YOUR JOB: 实现 sys_spawn 系统调用
// ALERT: 注意在实现 SPAWN 时不需要复制父进程地址空间，SPAWN != FORK + EXEC 
pub fn sys_spawn(_path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, _path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        let new_task = task.spawn(all_data.as_slice());
        let new_pid = new_task.pid.0;
        let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
        trap_cx.x[10] = 0;
        add_task(new_task);
        new_pid as isize
    } else {
        -1
    }
}
