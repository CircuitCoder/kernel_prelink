use elf_rs::{ElfFile, SectionHeaderFlags, SectionType};

use crate::{elf::Dynamic, mem::{VirtAddr, PhysAddr}};

pub trait Page : Clone + Copy {
    fn inner(&self) -> &'static [u8; 4096];
}

#[derive(Clone, Copy)]
pub struct Perm {
    pub r: bool,
    pub w: bool,
    pub x: bool,
}

pub trait MMU {
    type AllocatedPage : Page;
    fn alloc(&mut self) -> Self::AllocatedPage;
    fn map(&mut self, page: Self::AllocatedPage, vpn: usize, perm: Perm);
    fn map_existing(&mut self, ppn: usize, vpn: usize, perm: Perm);
    fn translate(&self, paddr: usize) -> Option<usize>;
}

#[derive(PartialEq, Eq)]
struct LDSOPageRange {
    start: usize,
    end: usize,
    target: usize,
}

pub struct StackConfig {
    start: usize,
    end: usize
}

pub struct Loader {
    entry: usize,
}

impl Loader {
    fn load<M: MMU, const ldso: Option<LDSOPageRange>, F>(buf: &[u8], mmu: &mut M, mut lookup: F, stack: StackConfig) -> Loader
    where
        F: for<'r> FnMut(&'r [u8]) -> Option<usize>
    {
        let parsed = elf_rs::Elf64::from_bytes(buf).unwrap();
        let header = parsed.elf_header();

        let mut dynamic = None;

        // Allocate memories
        for sec_hdr in parsed.section_header_iter() {
            if sec_hdr.section_name().starts_with(b".dynamic") {
                dynamic = Some(Dynamic::parse(buf, sec_hdr.offset() as usize .. (sec_hdr.offset()  + sec_hdr.size()) as usize));
            }

            if !sec_hdr.flags().contains(SectionHeaderFlags::SHF_ALLOC) {
                continue;
            }

            let addr = sec_hdr.addr() as usize;
            let size = sec_hdr.size() as usize;
            assert!(size > 0);

            let src = if sec_hdr.sh_type() != SectionType::SHT_NOBITS {
                let offset = sec_hdr.offset() as usize;
                let content = &buf[offset..(offset + size)];
                Some(content)
            } else {
                None
            };

            let virt_start: usize = VirtAddr(addr).floor().number();
            let virt_end: usize = VirtAddr(addr + size).ceil().number();
            let perm = Perm {
                r: true,
                w: sec_hdr.flags().contains(SectionHeaderFlags::SHF_WRITE),
                x: sec_hdr.flags().contains(SectionHeaderFlags::SHF_EXECINSTR),
            };

            // Alloc pages
            for vpn in virt_start .. virt_end {
                let page = mmu.alloc();
                // TODO: copy pages
                mmu.map(page, vpn, perm);
            }
        }

        // Map VDSO text
        if let Some(config) = ldso {
            let text_vdso_start_ppn = PhysAddr(config.start).floor().0;
            let text_vdso_end_ppn = PhysAddr(config.end).ceil().0;
            let text_vdso_start_vpn = VirtAddr(config.target).floor().0;

            let perm = Perm {
                x: true,
                r: true,
                w: false,
            };

            for ppn in text_vdso_start_ppn .. text_vdso_end_ppn {
                let pcount = ppn - text_vdso_start_ppn;
                let vpn = text_vdso_start_vpn + pcount;
                mmu.map_existing(ppn, vpn, perm);
            }

            if let Some(dynamic) = &dynamic {
                if let Some(inner) = &dynamic.rel {
                    match &inner {
                        crate::elf::RelTable::RELA(tbl) => {
                            for ent in *tbl {
                                let (sym, name) = dynamic.resolve_sym(ent.info >> 32);
                                if let Some(at) = lookup(name) {
                                    // Found, fill in GOT
                                    let target_offset = at - config.start as usize;
                                    let target_vaddr = config.target + target_offset;
                                    let got_vaddr = ent.offset;
                                    let got_paddr = mmu.translate(got_vaddr.into()).unwrap();
                                    unsafe { (got_paddr as *mut usize).write(target_vaddr) };
                                }
                            }
                        },
                        crate::elf::RelTable::REL(_) => todo!(),
                    }
                }
            }
        }

        // Fixup GOT

        // Allocate stack

        // TODO: extendable stack
        let stack_end = VirtAddr(stack.end).ceil().number();
        let stack_start = VirtAddr(stack.start).floor().number();
        let stack_perm = Perm {
            r: true,
            w: true,
            x: false,
        };

        for stack_vpn in stack_start .. stack_end {
            let page = mmu.alloc();
            mmu.map(page, stack_vpn, stack_perm);
        }
        // let stack_area = MapArea::frames(stack_start .. stack_end, MapPermission::U | MapPermission::W | MapPermission::R);
        // mset.push(stack_area, None);

        let entry = parsed.entry_point() as usize;
        // mprintln!("Entry: {:#x}", entry);
        // let tf = TrapFrame::with_process(true, entry, USER_STACK_TOP);

        Loader {
            entry,
        }
    }
}