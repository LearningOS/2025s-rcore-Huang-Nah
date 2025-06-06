//! Process management syscalls
use crate::{
    config::PAGE_SIZE,
    mm::{translated_byte_buffer, translated_byte_buffer_read, translated_byte_buffer_write, MapPermission},
    task::{
        change_program_brk, current_user_token, exit_current_and_run_next, get_syscall_count,
        suspend_current_and_run_next, current_task_mmap, current_task_munmap,
    },
    timer::get_time_us,
};


#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    let timeval = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };

    let buffers = translated_byte_buffer(
        current_user_token(),
        _ts as *const u8,
        core::mem::size_of::<TimeVal>(),
    );

    if buffers.is_empty() {
        return -1;
    }

    // transalte the TimeVal to array (byte)
    let timeval_bytes = unsafe {
        core::slice::from_raw_parts(
            &timeval as *const TimeVal as *const u8,
            core::mem::size_of::<TimeVal>(),
        )
    };

    //copy the statics into the user-mode memory, ensure permission isolation.
    let mut pos = 0;
    for buffer in buffers {
        for i in 0..buffer.len() {
            if pos < timeval_bytes.len() {
                buffer[i] = timeval_bytes[pos];
                pos += 1;
            }
        }
    }

    0
}

/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(_trace_request: usize, _id: usize, _data: usize) -> isize {
    trace!("kernel: sys_trace");
    match _trace_request {
        0 => {
            // 读取：从用户地址读取一个字节
            let buffer_now = translated_byte_buffer_read(current_user_token(), _id as *const u8, 1);
            if buffer_now.is_empty() {
                return -1;
            }
            buffer_now[0][0] as isize
        }
        1 => {
            // 写入：向用户地址写入一个字节
            let mut address_buff =
                translated_byte_buffer_write(current_user_token(), _id as *const u8, 1);
            if address_buff.is_empty() {
                return -1;
            }
            address_buff[0][0] = _data as u8;
            0
        }
        2 => {
            // 获取系统调用计数
            let _id_index = match _id {
                64 => 0,
                93 => 1,
                124 => 2,
                169 => 3,
                410 => 4,
                222 => 5,
                215 => 6,
                214 => 7,
                _ => return -1, // 无效的系统调用ID
            };
            get_syscall_count(_id_index) as isize
        }
        _ => -1,
    }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _prot: usize) -> isize {
    trace!("kernel: sys_mmap");
    // start must be page aligned
    if _start % PAGE_SIZE != 0 {
        return -1;
    }

    // prot flags validation
    if _prot & !0x7 != 0 {
        return -1;
    }

    // prot cannot be 0
    if _prot & 0x7 == 0 {
        return -1;
    }

    // len of 0 is valid, just return success
    if _len == 0 {
        return 0;
    }

    // Convert prot flags to MapPermission
    let mut permissions = MapPermission::U;
    if _prot & 0x1 != 0 {
        permissions |= MapPermission::R;
    }
    if _prot & 0x2 != 0 {
        permissions |= MapPermission::W;
    }
    if _prot & 0x4 != 0 {
        permissions |= MapPermission::X;
    }
    
    current_task_mmap(_start, _len, permissions)
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap");
    // start must be page aligned
    if _start % PAGE_SIZE != 0 {
        return -1;
    }
    
    // len of 0 is valid, just return success
    if _len == 0 {
        return 0;
    }
    
    current_task_munmap(_start, _len)
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }

}
