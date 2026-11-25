use bebop_elf::CpuProfileScope;
use std::collections::HashMap;

pub struct Context {
    pub pc: u64,
    pub ra: u64,
    pub arguments: [u64; 8],
    pub instructions: u64,
    pub cycle: u64,
}

struct Active {
    scope: u64,
    call: u64,
    parent: Option<u64>,
    end: u64,
    start: u64,
    instructions: u64,
    cycle: u64,
    arguments: [u64; 8],
}

#[derive(serde::Serialize)]
pub struct Sample {
    pub scope: u64,
    pub call: u64,
    pub parent: Option<u64>,
    pub begin_cycle: u64,
    pub end_cycle: u64,
    pub modeled_active_cycles: u64,
    pub instructions: u64,
    pub arguments: [u64; 8],
    pub result: u64,
}

#[derive(Default)]
pub struct CallProfile {
    entries: HashMap<u64, CpuProfileScope>,
    stack: Vec<Active>,
    next_call: u64,
}

impl CallProfile {
    pub fn new(scopes: &[CpuProfileScope]) -> Self {
        let mut profile = Self::default();
        for scope in scopes {
            assert!(
                profile.entries.insert(scope.begin, *scope).is_none(),
                "CPU profile entries must be distinct"
            );
        }
        profile
    }

    pub fn interesting(&self, pc: u64) -> bool {
        (!self.entries.is_empty() && self.entries.contains_key(&pc))
            || self.stack.last().is_some_and(|active| active.end == pc)
    }

    pub fn observe(&mut self, context: Context, active_cycles: u64) -> Vec<Sample> {
        let mut samples = Vec::new();
        while self.stack.last().is_some_and(|active| active.end == context.pc) {
            let active = self.stack.pop().unwrap();
            samples.push(Sample {
                scope: active.scope,
                call: active.call,
                parent: active.parent,
                begin_cycle: active.cycle,
                end_cycle: context.cycle,
                modeled_active_cycles: active_cycles - active.start,
                instructions: context.instructions - active.instructions,
                arguments: active.arguments,
                result: context.arguments[0],
            });
        }
        if let Some(scope) = self.entries.get(&context.pc) {
            self.stack.push(Active {
                scope: scope.id,
                call: self.next_call,
                parent: self.stack.last().map(|a| a.call),
                end: if scope.end == 0 { context.ra } else { scope.end },
                start: active_cycles,
                instructions: context.instructions,
                cycle: context.cycle,
                arguments: context.arguments,
            });
            self.next_call += 1;
        }
        samples
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at(pc: u64, ra: u64, cycle: u64) -> Context {
        Context {
            pc,
            ra,
            arguments: [0; 8],
            instructions: cycle,
            cycle,
        }
    }

    #[test]
    fn nested_calls_count_inclusive_work_and_repeated_invocations() {
        let mut profile = CallProfile::new(&[
            CpuProfileScope {
                id: 1,
                begin: 100,
                end: 0,
            },
            CpuProfileScope {
                id: 2,
                begin: 200,
                end: 0,
            },
        ]);
        assert!(profile.observe(at(100, 400, 0), 0).is_empty());
        assert!(profile.observe(at(200, 300, 10), 10).is_empty());
        let nested = profile.observe(at(300, 0, 30), 25).pop().unwrap();
        assert_eq!(
            (nested.scope, nested.parent, nested.modeled_active_cycles),
            (2, Some(0), 15)
        );
        let outer = profile.observe(at(400, 0, 50), 40).pop().unwrap();
        assert_eq!(
            (outer.scope, outer.instructions, outer.modeled_active_cycles),
            (1, 50, 40)
        );
        profile.observe(at(100, 400, 60), 50);
        let repeated = profile.observe(at(400, 0, 80), 65).pop().unwrap();
        assert_eq!((repeated.call, repeated.modeled_active_cycles), (2, 15));
    }

    #[test]
    fn explicit_region_and_its_final_call_close_at_the_same_pc() {
        let mut profile = CallProfile::new(&[
            CpuProfileScope {
                id: 1,
                begin: 100,
                end: 400,
            },
            CpuProfileScope {
                id: 2,
                begin: 200,
                end: 0,
            },
        ]);
        profile.observe(at(100, 999, 0), 0);
        profile.observe(at(200, 400, 10), 10);
        let samples = profile.observe(at(400, 0, 1000), 30);
        assert_eq!(samples.len(), 2);
        assert_eq!(
            (samples[0].modeled_active_cycles, samples[1].modeled_active_cycles),
            (20, 30)
        );
        assert!(!profile.interesting(999));
    }
}
