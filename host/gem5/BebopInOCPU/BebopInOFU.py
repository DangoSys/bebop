from m5.objects.FuncUnit import OpClass
from m5.objects.TimingExpr import TimingExpr
from m5.params import *
from m5.SimObject import SimObject


class BebopInOOpClass(SimObject):
  type = "BebopInOOpClass"
  cxx_header = "BebopInOCPU/func_unit.hh"
  cxx_class = "gem5::BebopInOOpClass"

  opClass = Param.OpClass("op class to match")


class BebopInOOpClassSet(SimObject):
  type = "BebopInOOpClassSet"
  cxx_header = "BebopInOCPU/func_unit.hh"
  cxx_class = "gem5::BebopInOOpClassSet"

  opClasses = VectorParam.BebopInOOpClass([], "op classes to match")


class BebopInOFUTiming(SimObject):
  type = "BebopInOFUTiming"
  cxx_header = "BebopInOCPU/func_unit.hh"
  cxx_class = "gem5::BebopInOFUTiming"

  mask = Param.UInt64(0, "mask for matching ExtMachInst")
  match = Param.UInt64(0, "match value for ExtMachInst")
  suppress = Param.Bool(False, "if true, suppress this inst")
  extraCommitLat = Param.Cycles(0, "extra cycles at commit")
  extraCommitLatExpr = Param.TimingExpr(NULL, "extra commit cycles expr")
  extraAssumedLat = Param.Cycles(0, "extra assumed latency")
  srcRegsRelativeLats = VectorParam.Cycles(
    [], "per-src-reg relative latencies"
  )
  opClasses = Param.BebopInOOpClassSet(
    BebopInOOpClassSet(), "op classes to apply timing to"
  )
  description = Param.String("", "description string")


class BebopInOFU(SimObject):
  type = "BebopInOFU"
  cxx_header = "BebopInOCPU/func_unit.hh"
  cxx_class = "gem5::BebopInOFU"

  opClasses = Param.BebopInOOpClassSet(
    BebopInOOpClassSet(), "op classes supported"
  )
  opLat = Param.Cycles(1, "operation latency")
  opLatExpr = Param.TimingExpr(NULL, "latency expression")
  issueLat = Param.Cycles(1, "issue latency")
  timings = VectorParam.BebopInOFUTiming([], "extra timing info")
  cantForwardFromFUIndices = VectorParam.Unsigned(
    [], "FU indices this FU can't forward from"
  )


class BebopInOFUPool(SimObject):
  type = "BebopInOFUPool"
  cxx_header = "BebopInOCPU/func_unit.hh"
  cxx_class = "gem5::BebopInOFUPool"

  funcUnits = VectorParam.BebopInOFU("functional units")
