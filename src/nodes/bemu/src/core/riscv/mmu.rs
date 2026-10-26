use crate::core::riscv::{
    bus::{Bus, Width},
    pmp::Pmp,
    Access, Privilege, Trap,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

const PPN_MASK: u64 = (1 << 44) - 1;
const MPRV: u64 = 1 << 17;
const SUM: u64 = 1 << 18;
const MXR: u64 = 1 << 19;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Satp {
    #[default]
    Bare,
    Sv39 {
        asid: u16,
        ppn: u64,
    },
}

impl Satp {
    pub fn read(self) -> u64 {
        match self {
            Self::Bare => 0,
            Self::Sv39 { asid, ppn } => 8 << 60 | u64::from(asid) << 44 | ppn,
        }
    }

    pub fn write(&mut self, value: u64) {
        match value >> 60 {
            0 => *self = Self::Bare,
            8 => {
                *self = Self::Sv39 {
                    asid: (value >> 44) as u16,
                    ppn: value & PPN_MASK,
                }
            }
            // The architecture requires an unsupported MODE write to leave satp unchanged.
            _ => (),
        }
    }
}

pub struct TranslationContext<'a> {
    pub privilege: Privilege,
    pub satp: Satp,
    pub mstatus: u64,
    pub pmp: &'a Pmp,
}

impl TranslationContext<'_> {
    pub(crate) fn effective_privilege(&self, access: Access) -> Privilege {
        if access != Access::Fetch && self.privilege == Privilege::Machine && self.mstatus & MPRV != 0 {
            match (self.mstatus >> 11) & 3 {
                0 => Privilege::User,
                1 => Privilege::Supervisor,
                3 => Privilege::Machine,
                _ => panic!("reserved mstatus.MPP"),
            }
        } else {
            self.privilege
        }
    }
}

pub struct Mmu {
    cache: Mutex<Cache>,
    generation: AtomicU64,
}

static GENERATION: AtomicU64 = AtomicU64::new(1);

impl Default for Mmu {
    fn default() -> Self {
        Self {
            cache: Mutex::new(Cache::default()),
            generation: AtomicU64::new(GENERATION.fetch_add(1, Ordering::Relaxed)),
        }
    }
}

struct Context {
    satp: Satp,
    status: u64,
    privilege: Privilege,
    pmp_generation: u64,
}

#[derive(Clone, Copy)]
struct Page {
    virtual_page: u64,
    physical_page: u64,
    access: Access,
}

struct Cache {
    context: Option<Context>,
    pages: [Option<Page>; 128],
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            context: None,
            pages: [None; 128],
        }
    }
}

impl Mmu {
    pub fn fence(&self) {
        let mut cache = self.cache.lock().expect("MMU cache poisoned");
        cache.pages.fill(None);
        self.generation
            .store(GENERATION.fetch_add(1, Ordering::Relaxed), Ordering::Release);
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub fn translate(
        &self,
        bus: &mut impl Bus,
        context: &TranslationContext<'_>,
        address: u64,
        width: Width,
        access: Access,
    ) -> Result<u64, Trap> {
        if address & (width as u64 - 1) != 0 {
            return Err(access.misaligned(address));
        }
        let mode = context.effective_privilege(access);
        let physical = if mode == Privilege::Machine || context.satp == Satp::Bare {
            address
        } else {
            let mut cache = self.cache.lock().expect("MMU cache poisoned");
            let status = context.mstatus & (SUM | MXR);
            if cache.context.as_ref().is_none_or(|old| {
                old.satp != context.satp
                    || old.status != status
                    || old.privilege != mode
                    || old.pmp_generation != context.pmp.generation()
            }) {
                cache.pages.fill(None);
                cache.context = Some(Context {
                    satp: context.satp,
                    status,
                    privilege: mode,
                    pmp_generation: context.pmp.generation(),
                });
            }
            let virtual_page = address >> 12;
            let slot = ((virtual_page as usize)
                ^ match access {
                    Access::Fetch => 0,
                    Access::Load => 43,
                    Access::Store => 86,
                })
                & 127;
            let physical_page = match cache.pages[slot] {
                Some(page) if page.virtual_page == virtual_page && page.access == access => page.physical_page,
                _ => {
                    let physical_page = self.walk(bus, context, mode, address, access)? >> 12;
                    cache.pages[slot] = Some(Page {
                        virtual_page,
                        physical_page,
                        access,
                    });
                    physical_page
                }
            };
            physical_page << 12 | (address & 0xfff)
        };
        if physical >> 56 != 0 || !context.pmp.allows(physical, width, access, mode) {
            return Err(access.fault(address));
        }
        Ok(physical)
    }

    fn walk(
        &self,
        bus: &mut impl Bus,
        context: &TranslationContext<'_>,
        mode: Privilege,
        address: u64,
        access: Access,
    ) -> Result<u64, Trap> {
        if ((address << 25) as i64 >> 25) as u64 != address {
            return Err(access.page_fault(address));
        }
        let Satp::Sv39 { ppn: root, .. } = context.satp else {
            unreachable!()
        };
        // A competing page-table writer can change the PTE before the A/D update.
        // Restarting the walk after a failed CAS is required architectural behavior.
        'retry: loop {
            let mut table = root << 12;
            for level in (0..3).rev() {
                let shift = 12 + 9 * level;
                let pte_address = table + ((address >> shift) & 0x1ff) * 8;
                if !context
                    .pmp
                    .allows(pte_address, Width::Double, Access::Load, Privilege::Supervisor)
                {
                    return Err(access.fault(address));
                }
                let pte = bus
                    .read(pte_address, Width::Double)
                    .map_err(|_| access.fault(address))?;
                if pte & 1 == 0 || pte & 6 == 4 || pte >> 54 != 0 {
                    return Err(access.page_fault(address));
                }
                let ppn = (pte >> 10) & PPN_MASK;
                if pte & 0xa == 0 {
                    if pte & 0xd0 != 0 || level == 0 {
                        return Err(access.page_fault(address));
                    }
                    table = ppn << 12;
                    continue;
                }
                let user = pte & 0x10 != 0;
                if (mode == Privilege::User && !user)
                    || (mode == Privilege::Supervisor
                        && user
                        && (access == Access::Fetch || context.mstatus & SUM == 0))
                {
                    return Err(access.page_fault(address));
                }
                let permitted = match access {
                    Access::Fetch => pte & 8 != 0,
                    Access::Load => pte & 2 != 0 || (context.mstatus & MXR != 0 && pte & 8 != 0),
                    Access::Store => pte & 4 != 0,
                };
                let low_ppn = (1u64 << (9 * level)) - 1;
                if !permitted || ppn & low_ppn != 0 {
                    return Err(access.page_fault(address));
                }
                let ad = 0x40 | if access == Access::Store { 0x80 } else { 0 };
                if pte & ad != ad {
                    if !context
                        .pmp
                        .allows(pte_address, Width::Double, Access::Store, Privilege::Supervisor)
                    {
                        return Err(access.fault(address));
                    }
                    let previous = bus
                        .compare_exchange(pte_address, Width::Double, pte, pte | ad)
                        .map_err(|_| access.fault(address))?;
                    if previous != pte {
                        continue 'retry;
                    }
                }
                return Ok((ppn << 12) | (address & ((1u64 << shift) - 1)));
            }
            unreachable!();
        }
    }
}
