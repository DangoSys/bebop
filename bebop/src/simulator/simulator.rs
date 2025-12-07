use crate::buckyball::buckyball::Buckyball;
use super::sim::mode::SimMode;
use std::io;

/// Simulator - 对外的模拟器接口
/// 
/// 这是一个包装层，将底层的Buckyball仿真引擎封装为对外API
pub struct Simulator {
    buckyball: Buckyball,
}

impl Simulator {
    /// 创建新的模拟器实例
    pub fn new(mode: SimMode) -> Self {
        Self {
            buckyball: Buckyball::new(mode),
        }
    }

    /// 注入一条指令
    pub fn inject_instruction(&mut self, instruction: &str) {
        self.buckyball.inject_instruction(instruction);
    }

    /// 运行模拟器
    pub fn run(&mut self) -> io::Result<()> {
        self.buckyball.run()
    }

    /// 单步执行
    pub fn step(&mut self) -> io::Result<bool> {
        self.buckyball.step()
    }

    /// 获取当前模拟时间
    pub fn get_time(&self) -> f64 {
        self.buckyball.get_time()
    }
}
