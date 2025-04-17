//! Memory management implementation
//!
//! SV39 page-based virtual-memory architecture for RV64 systems, and
//! everything about memory management, like frame allocator, page table,
//! map area and memory set, is implemented here.
//!
//! Every task or process has a memory_set to control its virtual memory.

mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod page_table;

pub use address::{PhysAddr, PhysPageNum, VirtAddr, VirtPageNum};
use address::{StepByOne, VPNRange};
pub use frame_allocator::{frame_alloc, FrameTracker};
pub use memory_set::remap_test;
pub use memory_set::{kernel_stack_position, MapPermission, MemorySet, KERNEL_SPACE};
pub use page_table::{translated_byte_buffer, PageTableEntry};
use page_table::{PTEFlags, PageTable};

/// initiate heap allocator, frame allocator and kernel space
pub fn init() {
    heap_allocator::init_heap();
    frame_allocator::init_frame_allocator();
    KERNEL_SPACE.exclusive_access().activate();
}

/// Translate&Copy a ptr[u8] array with LENGTH len to a mutable u8 Vec through page table
pub fn trace_read<T>(token: usize, src: usize) -> Option<T> {
    debug!("trace_read: src {:x?}", src);
    
    let pt = PageTable::from_token(token);
    let start_va = VirtAddr::from(src);
    let vpn = start_va.floor();
    
    // Early return if translation fails or PTE checks fail
    let pte = pt.translate(vpn).filter(|pte| {
        pte.is_valid() && pte.user() && pte.readable()
    })?;
    
    debug!("trace_read: pte {:?}", pte.bits);
    
    let mut dst = core::mem::MaybeUninit::<T>::uninit();
    let len = core::mem::size_of::<T>();
    debug!("trace_read: len {:?}", len);
    
    let src_frames = translated_byte_buffer(token, src as *const u8, len);
    debug!("trace_read: src_frames {:?}", src_frames);
    
    // Copy data using fold to accumulate offset
    let _ = src_frames.iter().fold(0, |offset, src_frame| {
        let dst_ptr = dst.as_mut_ptr() as *mut u8;
        unsafe {
            core::ptr::copy_nonoverlapping(
                src_frame.as_ptr(),
                dst_ptr.add(offset),
                src_frame.len()
            );
        }
        offset + src_frame.len()
    });
    
    Some(unsafe { dst.assume_init() })
}

/// Write data to the virtual address `dst` in the user space
pub fn trace_write<T>(token: usize, data: T, dst: usize) -> bool {
    debug!("trace_write: dst {:x?}", dst);
    
    let pt = PageTable::from_token(token);
    let start_va = VirtAddr::from(dst);
    let vpn = start_va.floor();
    
    // Early return if translation fails or PTE checks fail
    let pte = pt.translate(vpn).filter(|pte| {
        pte.is_valid() && pte.user() && pte.writable()
    });
    if pte.is_none() {
        return false;
    }
    
    let len = core::mem::size_of::<T>();
    let dst_frames = translated_byte_buffer(token, dst as *const u8, len);
    
    // Copy data using fold to accumulate offset
    let src_ptr = &data as *const T as *const u8;
    let _ = dst_frames.iter().fold(0, |offset, dst_frame| {
        unsafe {
            core::ptr::copy_nonoverlapping(
                src_ptr.add(offset),
                dst_frame.as_ptr() as *mut u8,
                dst_frame.len()
            );
        }
        offset + dst_frame.len()
    });
    
    true
}