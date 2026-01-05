import asyncio

config = {
    "type": "event",
    "name": "decoder",
    "description": "clean build directory",
    "subscribes": ["frontend.decode", "frontend.rob.retry", "frontend.rob.allocated"],
    "emits": ["frontend.rob.allocate"],
    "flows": ["mvin"],
}

# Decoder state - track pending retry requests
decoder_state = {
    "blocked": False,
    "pending_retry": None  # {funct, xs1, xs2, domain_id}
}


async def handler(data, context):
    # Check if this is an allocated event (success notification from ROB)
    if "rob_id" in data:
        # Only process if decoder is blocked (waiting for retry to succeed)
        if decoder_state["blocked"] and decoder_state["pending_retry"]:
            funct, xs1, xs2, domain_id = data.get("funct"), data.get("xs1"), data.get("xs2"), data.get("domain_id")
            # Verify this matches the pending retry
            pending = decoder_state["pending_retry"]
            if (pending["funct"] == funct and pending["xs1"] == xs1 and 
                pending["xs2"] == xs2 and pending["domain_id"] == domain_id):
                print(f"ROB allocation succeeded, clearing block: rob_id={data.get('rob_id')}")
                decoder_state["blocked"] = False
                decoder_state["pending_retry"] = None
                return {"message": "ROB allocation confirmed, decoder unblocked"}
        # If not blocked, ignore this event (first-time success doesn't need notification)
        return {"message": "ROB allocation confirmed"}
    
    # Check if this is a retry event (has domain_id but no rob_id)
    if "domain_id" in data and "rob_id" not in data:
        # This is a retry event from ROB
        funct, xs1, xs2, domain_id = data.get("funct"), data.get("xs1"), data.get("xs2"), data.get("domain_id")
        print(f"Retrying ROB allocation: funct={funct}, xs1={xs1}, xs2={xs2}, domain_id={domain_id}")
        decoder_state["blocked"] = True
        decoder_state["pending_retry"] = {"funct": funct, "xs1": xs1, "xs2": xs2, "domain_id": domain_id}
        await context.emit({"topic": "frontend.rob.allocate", "data": {"funct": funct, "xs1": xs1, "xs2": xs2, "domain_id": domain_id}})
        return {"message": "ROB allocation retried"}
    
    # New decode request (no domain_id, no rob_id)
    if decoder_state["blocked"]:
        # Decoder is blocked, reject new requests
        error_msg = "Decoder is blocked, waiting for ROB retry to complete"
        print(error_msg)
        raise RuntimeError(error_msg)
    
    funct, xs1, xs2 = data.get("funct"), data.get("xs1"), data.get("xs2")
    print(f"Decoding instruction: funct={funct}, xs1={xs1}, xs2={xs2}")
    match funct:
        case 31:
            domain_id = 0
        case 24 | 25:
            domain_id = 1
        case _:
            domain_id = 2
    
    await context.emit({"topic": "frontend.rob.allocate", "data": {"funct": funct, "xs1": xs1, "xs2": xs2, "domain_id": domain_id}})
    return {"message": "Instruction decoded"}
