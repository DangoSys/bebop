from m5.defines import buildEnv

from m5.objects.RiscvCPU import RiscvCPU, RiscvMMU
from m5.objects.BaseBebopInOCPU import BaseBebopInOCPU


if buildEnv.get("USE_ARM_ISA"):
  from m5.objects.ArmCPU import ArmCPU, ArmMMU

  class ArmBebopInOCPU(BaseBebopInOCPU, ArmCPU):
    type = "ArmBebopInOCPU"
    cxx_header = "BebopInOCPU/cpu.hh"
    cxx_class = "gem5::BebopInOCPU"
    mmu = ArmMMU()


if buildEnv.get("USE_RISCV_ISA"):
  class RiscvBebopInOCPU(BaseBebopInOCPU, RiscvCPU):
    type = "RiscvBebopInOCPU"
    cxx_header = "BebopInOCPU/cpu.hh"
    cxx_class = "gem5::BebopInOCPU"
    mmu = RiscvMMU()

