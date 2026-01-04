import asyncio

config = {
    "type": "api",
    "name": "global rob",
    "description": "allocate ROB entry for instruction",
    "path": "/frontend/rob/allocate",
    "method": "POST",
    "emits": ["frontend.rob.allocate"],
    "flows": ["mvin"],
}

async def handler(req, context):
    body = req.get("body") or {}
    funct, xs1, xs2, domain_id = body.get("funct"), body.get("xs1"), body.get("xs2"), body.get("domain_id")

    if funct is None or xs1 is None or xs2 is None or domain_id is None:
        return {"error": "Missing required fields"}

    await context.emit({
        "topic": "frontend.rob.allocate",
        "data": {
            "funct": funct,
            "xs1": xs1,
            "xs2": xs2,
            "domain_id": domain_id
        }
    })

    return {
        "status": 200,
        "message": "ROB allocation requested"
    }
