from m5.defines import buildEnv
from m5.objects.BaseCPU import BaseCPU
from m5.objects.BranchPredictor import *
from m5.objects.DummyChecker import DummyChecker
from m5.objects.FuncUnit import OpClass
from m5.objects.TimingExpr import TimingExpr
from m5.objects.BebopInOFU import (
  BebopInOOpClass,
  BebopInOOpClassSet,
  BebopInOFUTiming,
  BebopInOFU,
  BebopInOFUPool,
)
from m5.params import *
from m5.proxy import *
from m5.SimObject import SimObject


def bebopMakeOpClassSet(op_classes):
  def boxOpClass(op_class):
    return BebopInOOpClass(opClass=op_class)

  return BebopInOOpClassSet(opClasses=[boxOpClass(o) for o in op_classes])


class BebopInODefaultIntFU(BebopInOFU):
  opClasses = bebopMakeOpClassSet(["IntAlu"])
  timings = [BebopInOFUTiming(description="Int", srcRegsRelativeLats=[2])]
  opLat = 3


class BebopInODefaultIntMulFU(BebopInOFU):
  opClasses = bebopMakeOpClassSet(["IntMult"])
  timings = [BebopInOFUTiming(description="Mul", srcRegsRelativeLats=[0])]
  opLat = 3


class BebopInODefaultIntDivFU(BebopInOFU):
  opClasses = bebopMakeOpClassSet(["IntDiv"])
  issueLat = 9
  opLat = 9


class BebopInODefaultFloatSimdFU(BebopInOFU):
  opClasses = bebopMakeOpClassSet(
    [
      "FloatAdd",
      "FloatCmp",
      "FloatCvt",
      "FloatMisc",
      "FloatMult",
      "FloatMultAcc",
      "FloatDiv",
      "FloatSqrt",
      "SimdAdd",
      "SimdAddAcc",
      "SimdAlu",
      "SimdCmp",
      "SimdCvt",
      "SimdMisc",
      "SimdMult",
      "SimdMultAcc",
      "SimdMatMultAcc",
      "SimdShift",
      "SimdShiftAcc",
      "SimdDiv",
      "SimdSqrt",
      "SimdFloatAdd",
      "SimdFloatAlu",
      "SimdFloatCmp",
      "SimdFloatCvt",
      "SimdFloatDiv",
      "SimdFloatMisc",
      "SimdFloatMult",
      "SimdFloatMultAcc",
      "SimdFloatMatMultAcc",
      "SimdFloatSqrt",
      "SimdReduceAdd",
      "SimdReduceAlu",
      "SimdReduceCmp",
      "SimdFloatReduceAdd",
      "SimdFloatReduceCmp",
      "SimdAes",
      "SimdAesMix",
      "SimdSha1Hash",
      "SimdSha1Hash2",
      "SimdSha256Hash",
      "SimdSha256Hash2",
      "SimdShaSigma2",
      "SimdShaSigma3",
      "SimdPredAlu",
      "Matrix",
      "MatrixMov",
      "MatrixOP",
      "SimdExt",
      "SimdFloatExt",
      "SimdConfig",
    ]
  )

  timings = [
    BebopInOFUTiming(description="FloatSimd", srcRegsRelativeLats=[2])
  ]
  opLat = 6


class BebopInODefaultPredFU(BebopInOFU):
  opClasses = bebopMakeOpClassSet(["SimdPredAlu"])
  timings = [BebopInOFUTiming(description="Pred", srcRegsRelativeLats=[2])]
  opLat = 3


class BebopInODefaultMemFU(BebopInOFU):
  opClasses = bebopMakeOpClassSet(
    [
      "MemRead",
      "MemWrite",
      "FloatMemRead",
      "FloatMemWrite",
      "SimdUnitStrideLoad",
      "SimdUnitStrideStore",
      "SimdUnitStrideMaskLoad",
      "SimdUnitStrideMaskStore",
      "SimdStridedLoad",
      "SimdStridedStore",
      "SimdIndexedLoad",
      "SimdIndexedStore",
      "SimdUnitStrideFaultOnlyFirstLoad",
      "SimdWholeRegisterLoad",
      "SimdWholeRegisterStore",
    ]
  )
  timings = [
    BebopInOFUTiming(
      description="Mem", srcRegsRelativeLats=[1], extraAssumedLat=2
    )
  ]
  opLat = 1


class BebopInODefaultMiscFU(BebopInOFU):
  opClasses = bebopMakeOpClassSet(["InstPrefetch"])
  opLat = 1


class BebopInODefaultFUPool(BebopInOFUPool):
  funcUnits = [
    BebopInODefaultIntFU(),
    BebopInODefaultIntFU(),
    BebopInODefaultIntMulFU(),
    BebopInODefaultIntDivFU(),
    BebopInODefaultFloatSimdFU(),
    BebopInODefaultPredFU(),
    BebopInODefaultMemFU(),
    BebopInODefaultMiscFU(),
  ]


class BaseBebopInOCPU(BaseCPU):
  type = "BaseBebopInOCPU"
  cxx_header = "BebopInOCPU/cpu.hh"
  cxx_class = "gem5::BebopInOCPU"

  @classmethod
  def memory_mode(cls):
    return "timing"

  @classmethod
  def require_caches(cls):
    return True

  @classmethod
  def support_take_over(cls):
    return True

  threadPolicy = Param.ThreadPolicy("RoundRobin", "Thread scheduling policy")
  fetch1FetchLimit = Param.Unsigned(
    1, "Number of line fetches allowable in flight at once"
  )
  fetch1LineSnapWidth = Param.Unsigned(
    0,
    "Fetch1 'line' fetch snap size in bytes"
    " (0 means use system cache line size)",
  )
  fetch1LineWidth = Param.Unsigned(
    0,
    "Fetch1 maximum fetch size in bytes (0 means use system cache"
    " line size)",
  )
  fetch1ToFetch2ForwardDelay = Param.Cycles(
    1, "Forward cycle delay from Fetch1 to Fetch2 (1 means next cycle)"
  )
  fetch1ToFetch2BackwardDelay = Param.Cycles(
    1,
    "Backward cycle delay from Fetch2 to Fetch1 for branch prediction"
    " signalling (0 means in the same cycle, 1 mean the next cycle)",
  )

  fetch2InputBufferSize = Param.Unsigned(
    2, "Size of input buffer to Fetch2 in cycles-worth of insts."
  )
  fetch2ToDecodeForwardDelay = Param.Cycles(
    1, "Forward cycle delay from Fetch2 to Decode (1 means next cycle)"
  )
  fetch2CycleInput = Param.Bool(
    True,
    "Allow Fetch2 to cross input lines to generate full output each"
    " cycle",
  )

  decodeInputBufferSize = Param.Unsigned(
    3, "Size of input buffer to Decode in cycles-worth of insts."
  )
  decodeToExecuteForwardDelay = Param.Cycles(
    1, "Forward cycle delay from Decode to Execute (1 means next cycle)"
  )
  decodeInputWidth = Param.Unsigned(
    2,
    "Width (in instructions) of input to Decode (and implicitly"
    " Decode's own width)",
  )
  decodeCycleInput = Param.Bool(
    True,
    "Allow Decode to pack instructions from more than one input cycle"
    " to fill its output each cycle",
  )

  executeInputWidth = Param.Unsigned(
    2, "Width (in instructions) of input to Execute"
  )
  executeCycleInput = Param.Bool(
    True,
    "Allow Execute to use instructions from more than one input cycle"
    " each cycle",
  )
  executeIssueLimit = Param.Unsigned(
    2, "Number of issuable instructions in Execute each cycle"
  )
  executeMemoryIssueLimit = Param.Unsigned(
    1, "Number of issuable memory instructions in Execute each cycle"
  )
  executeCommitLimit = Param.Unsigned(
    2, "Number of committable instructions in Execute each cycle"
  )
  executeMemoryCommitLimit = Param.Unsigned(
    1, "Number of committable memory references in Execute each cycle"
  )
  executeInputBufferSize = Param.Unsigned(
    7, "Size of input buffer to Execute in cycles-worth of insts."
  )
  executeMemoryWidth = Param.Unsigned(
    0,
    "Width (and snap) in bytes of the data memory interface. (0 mean use"
    " the system cacheLineSize)",
  )
  executeMaxAccessesInMemory = Param.Unsigned(
    2,
    "Maximum number of concurrent accesses allowed to the memory system"
    " from the dcache port",
  )
  executeLSQMaxStoreBufferStoresPerCycle = Param.Unsigned(
    2, "Maximum number of stores that the store buffer can issue per cycle"
  )
  executeLSQRequestsQueueSize = Param.Unsigned(
    1, "Size of LSQ requests queue (address translation queue)"
  )
  executeLSQTransfersQueueSize = Param.Unsigned(
    2, "Size of LSQ transfers queue (memory transaction queue)"
  )
  executeLSQStoreBufferSize = Param.Unsigned(5, "Size of LSQ store buffer")
  executeBranchDelay = Param.Cycles(
    1,
    "Delay from Execute deciding to branch and Fetch1 reacting"
    " (1 means next cycle)",
  )

  executeSetTraceTimeOnCommit = Param.Bool(
    True, "Set inst. trace times to be commit times"
  )
  executeSetTraceTimeOnIssue = Param.Bool(
    False, "Set inst. trace times to be issue times"
  )

  executeAllowEarlyMemoryIssue = Param.Bool(
    True,
    "Allow mem refs to be issued to the LSQ before reaching the head of"
    " the in flight insts queue",
  )

  enableIdling = Param.Bool(
    True, "Enable cycle skipping when the processor is idle\n"
  )

  branchPred = Param.BranchPredictor(
    BranchPredictor(
      conditionalBranchPred=TournamentBP(numThreads=Parent.numThreads)
    ),
    "Branch Predictor",
  )

  def addCheckerCpu(self):
    print("Checker not yet supported by BebopInOCPU")
    exit(1)

  # Functional unit pool
  executeFuncUnits = Param.BebopInOFUPool(
    BebopInODefaultFUPool(), "FU pool for this processor"
  )

