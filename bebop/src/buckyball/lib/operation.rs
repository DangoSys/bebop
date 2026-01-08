/// 如果这个单元是子模块，则修改self.xxx发生在ExternalOp.execute中；
/// 如果这个单元是主模块，则修改self.xxx发生在InternalOp.update中；

/// can_input只表示模型本身状态，不考虑外部控制信号
/// 外部控制信号单独约定实现，一般实现在ExternalOp中

/// ExternalOp
pub trait ExternalOp {
  type Input;
  fn can_input(&self, ctrl: bool) -> bool;
  fn has_input(&self, input: &Self::Input) -> bool;
  fn execute(&mut self, input: &Self::Input);
}

/// InternalOp
/// output: 不准修改self.xxx，只准返回self.xxx
pub trait InternalOp {
  type Output;
  fn has_output(&self) -> bool;
  fn update(&mut self);
  fn output(&mut self) -> Self::Output;
}
