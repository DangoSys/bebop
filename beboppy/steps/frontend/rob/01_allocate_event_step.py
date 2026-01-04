import asyncio

config = {
    "type": "event",
    "name": "rob",
    "description": "handle ROB allocation and commit",
    "subscribes": ["frontend.rob.allocate"],
    "emits": ["frontend.rs.dispatch"],
    "flows": ["mvin"],
}

# ROB state - maintain a counter for ROB entries
# In a real implementation, you'd track available entries, committed entries, etc.
rob_state = {
    "next_id": 0,
    "entries": {},  # rob_id -> {funct, xs1, xs2, domain_id, status}
    "max_entries": 64
}


async def handler(data, context):
    funct, xs1, xs2, domain_id = data.get("funct"), data.get("xs1"), data.get("xs2"), data.get("domain_id")

    # Check if ROB is full
    if len(rob_state["entries"]) >= rob_state["max_entries"]:
        print(f"ROB is full, cannot allocate new entry")
        return {"error": "ROB full"}

    # Allocate a new ROB ID
    rob_id = rob_state["next_id"]
    rob_state["next_id"] = (rob_state["next_id"] + 1) % (rob_state["max_entries"] * 2)

    # Store the entry
    rob_state["entries"][rob_id] = {
        "funct": funct,
        "xs1": xs1,
        "xs2": xs2,
        "domain_id": domain_id,
        "status": "allocated"
    }

    print(f"ROB allocated entry: rob_id={rob_id}, funct={funct}, xs1={xs1:#x}, xs2={xs2:#x}")


    # Emit ready signal with the allocated rob_id
    await context.emit({
        "topic": "frontend.rs.dispatch",
        "data": {
            "rob_id": rob_id,
            "funct": funct,
            "xs1": xs1,
            "xs2": xs2,
            "domain_id": domain_id
        }
    })

    return {
        "message": "ROB entry allocated",
        "rob_id": rob_id
    }