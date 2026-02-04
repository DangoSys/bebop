from m5.defines import buildEnv

from m5.objects.RiscvCPU import RiscvCPU, RiscvMMU
from m5.objects.BaseBebopInOCPU import BaseBebopInOCPU


if buildEnv.get("USE_ARM_ISA"):
  from m5.objects.ArmCPU import ArmCPU, ArmMMU

  class ArmBebopInOCPU(BaseBebopInOCPU, ArmCPU):
    mmu = ArmMMU()


if buildEnv.get("USE_RISCV_ISA"):
  class RiscvBebopInOCPU(BaseBebopInOCPU, RiscvCPU):
    mmu = RiscvMMU()

