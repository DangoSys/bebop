#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Privilege {
    User = 0,
    Supervisor = 1,
    Machine = 3,
}
