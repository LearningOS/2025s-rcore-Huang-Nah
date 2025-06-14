//! File and filesystem-related syscalls

use alloc::sync::Arc;

use crate::fs::{open_file, OpenFlags, Stat, ROOT_INODE};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer};
use crate::task::{current_task, current_user_token};

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(_fd: usize, _st: *mut Stat) -> isize {
    trace!(
        "kernel:pid[{}] sys_fstat",
        current_task().unwrap().pid.0
    );
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if _fd >= inner.fd_table.len(){
        return -1;
    }
    if let Some(_file) = &inner.fd_table[_fd]{
        let stat = Stat{
            dev:0, 
            ino:_fd as u64,
            mode: crate::fs::StatMode::FILE,
            nlink:1,
            pad:[0;7],
        };
        let token = current_user_token();
        drop(inner); // release lock before translation
        // Don't forget the function of translated_byte_buffer(translate the virtual page to physical page via PageTable)
        let stat_buffer = translated_byte_buffer(
            token,
            _st as *const u8,
            core::mem::size_of::<Stat>()
        );

        // as data is right now in the kernel part of the physical page, now translate into the user mode, first use from_raw_parts to converse in to format in the memory
        let stat_bytes = unsafe {
            core::slice::from_raw_parts(
                &stat as *const Stat as *const u8,
                core::mem::size_of::<Stat>()
            )
        };
        
        // copy the data above in to the user part
        let mut offset = 0;
        for buffer_slice in stat_buffer {
            for byte in buffer_slice {
                if offset < stat_bytes.len() {
                    *byte = stat_bytes[offset];
                    offset += 1;
                }
            }
        }
        
        0  // 成功
    } else {
        -1  // 文件描述符无效
    }
}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(_old_name: *const u8, _new_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_linkat",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let old_name = translated_str(token, _old_name);
    let new_name = translated_str(token, _new_name);
    
    // Check if old file exists
    if let Some(_old_inode) = ROOT_INODE.find(old_name.as_str()) {
        // Check if new file already exists
        if ROOT_INODE.find(new_name.as_str()).is_some() {
            return -1; // New file already exists
        }
        
        // For simplicity, create the new file as a copy (not a true hard link)
        // In a real filesystem, this would create a new directory entry pointing to the same inode
        if let Some(old_file) = open_file(old_name.as_str(), OpenFlags::RDONLY) {
            let data = old_file.read_all();
            if let Some(new_file) = open_file(new_name.as_str(), OpenFlags::CREATE | OpenFlags::WRONLY) {
                // Write data directly
                new_file.write_at(0, &data);
                0 // Success
            } else {
                -1
            }
        } else {
            -1
        }
    } else {
        -1 // Old file doesn't exist
    }
}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_unlinkat",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let file_name = translated_str(token, _name);
    
    if let Some(inode) = open_file(file_name.as_str(), OpenFlags::RDONLY) {
        let ref_count = Arc::strong_count(&inode);
        if ref_count == 1 {
            inode.clear();
        }
        drop(inode);
        0
    } else {
        -1
    }
}
