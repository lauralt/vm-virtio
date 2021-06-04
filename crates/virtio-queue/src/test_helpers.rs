use crate::{Descriptor, VirtqUsedElem, Queue};
use vm_memory::{VolatileSlice, VolatileRef, VolatileMemory, GuestAddress, GuestMemoryMmap, GuestUsize, GuestMemory, Address, AddressValue};
use std::marker::PhantomData;
use std::mem;

// Represents a virtio descriptor in guest memory.
pub struct VirtqDesc<'a> {
    desc: VolatileSlice<'a>,
}

//#[macro_export]
macro_rules! offset_of {
        ($ty:ty, $field:ident) => {
            unsafe { &(*std::ptr::null::<$ty>()).$field as *const _ as usize }
        };
    }

impl<'a> VirtqDesc<'a> {
    pub fn new(dtable: &'a VolatileSlice<'a>, i: u16) -> Self {
        let desc = dtable
            .get_slice((i as usize) * Self::dtable_len(1), Self::dtable_len(1))
            .unwrap();
        VirtqDesc { desc }
    }

    // pub fn addr(&self) -> VolatileRef<u64> {
    //     self.desc.get_ref(offset_of!(Descriptor, addr())).unwrap()
    // }
    //
    // pub fn len(&self) -> VolatileRef<u32> {
    //     self.desc.get_ref(offset_of!(Descriptor, len())).unwrap()
    // }
    //
    // pub fn flags(&self) -> VolatileRef<u16> {
    //     self.desc.get_ref(offset_of!(Descriptor, flags())).unwrap()
    // }
    //
    // pub fn next(&self) -> VolatileRef<u16> {
    //     self.desc.get_ref(offset_of!(Descriptor, next())).unwrap()
    // }
    //
    // pub fn set(&self, addr: u64, len: u32, flags: u16, next: u16) {
    //     self.addr().store(addr);
    //     self.len().store(len);
    //     self.flags().store(flags);
    //     self.next().store(next);
    // }

    pub fn dtable_len(nelem: u16) -> usize {
        16 * nelem as usize
    }
}

// // Represents a virtio queue ring. The only difference between the used and available rings,
// // is the ring element type.
// pub struct VirtqRing<'a, T> {
//     ring: VolatileSlice<'a>,
//     start: GuestAddress,
//     qsize: u16,
//     _marker: PhantomData<*const T>,
// }
//
// impl<'a, T> VirtqRing<'a, T>
//     where
//         T: vm_memory::ByteValued,
// {
//     pub fn new(
//         start: GuestAddress,
//         mem: &'a GuestMemoryMmap,
//         qsize: u16,
//         alignment: GuestUsize,
//     ) -> Self {
//         assert_eq!(start.0 & (alignment - 1), 0);
//
//         let (region, addr) = mem.to_region_addr(start).unwrap();
//         let size = Self::ring_len(qsize);
//         let ring = region.get_slice(addr.0 a, size).unwrap();
//
//         let result = VirtqRing {
//             ring,
//             start,
//             qsize,
//             _marker: PhantomData,
//         };
//
//         result.flags().store(0);
//         result.idx().store(0);
//         result.event().store(0);
//         result
//     }
//
//     pub fn start(&self) -> GuestAddress {
//         self.start
//     }
//
//     pub fn end(&self) -> GuestAddress {
//         self.start.unchecked_add(self.ring.len() as GuestUsize)
//     }
//
//     pub fn flags(&self) -> VolatileRef<u16> {
//         self.ring.get_ref(0).unwrap()
//     }
//
//     pub fn idx(&self) -> VolatileRef<u16> {
//         self.ring.get_ref(2).unwrap()
//     }
//
//     fn ring_offset(i: u16) -> usize {
//         4 + mem::size_of::<T>() * (i as usize)
//     }
//
//     pub fn ring(&self, i: u16) -> VolatileRef<T> {
//         assert!(i < self.qsize);
//         self.ring.get_ref(Self::ring_offset(i)).unwrap()
//     }
//
//     pub fn event(&self) -> VolatileRef<u16> {
//         self.ring.get_ref(Self::ring_offset(self.qsize)).unwrap()
//     }
//
//     pub fn ring_len(qsize: u16) -> usize {
//         Self::ring_offset(qsize) + 2
//     }
// }
//
// pub type VirtqAvail<'a> = VirtqRing<'a, u16>;
// pub type VirtqUsed<'a> = VirtqRing<'a, VirtqUsedElem>;
//
// trait GuestAddressExt {
//     fn align_up(&self, x: GuestUsize) -> GuestAddress;
// }
// impl GuestAddressExt for GuestAddress {
//     fn align_up(&self, x: GuestUsize) -> GuestAddress {
//         Self((self.0 + (x - 1)) & !(x - 1))
//     }
// }
//
// pub struct VirtQueue<'a> {
//     pub start: GuestAddress,
//     pub dtable: VolatileSlice<'a>,
//     pub avail: VirtqAvail<'a>,
//     pub used: VirtqUsed<'a>,
// }
//
// impl<'a> VirtQueue<'a> {
//     // We try to make sure things are aligned properly :-s
//     pub fn new(start: GuestAddress, mem: &'a GuestMemoryMmap, qsize: u16) -> Self {
//         // power of 2?
//         assert!(qsize > 0 && qsize & (qsize - 1) == 0);
//
//         let (region, addr) = mem.to_region_addr(start).unwrap();
//         let dtable = region
//             .get_slice(addr, VirtqDesc::dtable_len(qsize))
//             .unwrap();
//
//         const AVAIL_ALIGN: GuestUsize = 2;
//
//         let avail_addr = start
//             .unchecked_add(VirtqDesc::dtable_len(qsize) as GuestUsize)
//             .align_up(AVAIL_ALIGN);
//         let avail = VirtqAvail::new(avail_addr, mem, qsize, AVAIL_ALIGN);
//
//         const USED_ALIGN: GuestUsize = 4;
//
//         let used_addr = avail.end().align_up(USED_ALIGN);
//         let used = VirtqUsed::new(used_addr, mem, qsize, USED_ALIGN);
//
//         VirtQueue {
//             start,
//             dtable,
//             avail,
//             used,
//         }
//     }
//
//     pub fn size(&self) -> u16 {
//         (self.dtable.len() / VirtqDesc::dtable_len(1)) as u16
//     }
//
//     pub fn dtable(&self, i: u16) -> VirtqDesc {
//         VirtqDesc::new(&self.dtable, i)
//     }
//
//     pub fn dtable_start(&self) -> GuestAddress {
//         self.start
//     }
//
//     pub fn avail_start(&self) -> GuestAddress {
//         self.avail.start()
//     }
//
//     pub fn used_start(&self) -> GuestAddress {
//         self.used.start()
//     }
//
//     // Creates a new Queue, using the underlying memory regions represented by the VirtQueue.
//     pub fn create_queue(&self, mem: &'a GuestMemoryMmap) -> Queue<&'a GuestMemoryMmap> {
//         let mut q = Queue::new(mem, self.size());
//
//         q.size = self.size();
//         q.ready = true;
//         q.desc_table = self.dtable_start();
//         q.avail_ring = self.avail_start();
//         q.used_ring = self.used_start();
//
//         q
//     }
//
//     pub fn start(&self) -> GuestAddress {
//         self.dtable_start()
//     }
//
//     pub fn end(&self) -> GuestAddress {
//         self.used.end()
//     }
// }
