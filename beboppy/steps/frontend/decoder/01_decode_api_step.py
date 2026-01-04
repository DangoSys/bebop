import asyncio

config = {
    "type": "api",
    "name": "global decoder",
    "description": "decode instruction",
    "path": "/frontend/decode",
    "method": "POST",
    "emits": ["frontend.decode"],
    "flows": ["mvin"],
}

async def handler(req, context):
    body = req.get("body") or {}
    func, xs1, xs2 = body.get("funct"), body.get("xs1"), body.get("xs2")
    if func is None or xs1 is None or xs2 is None:
        return {"error": "Missing required fields"}
    await context.emit({"topic": "frontend.decode", "data": {"funct": func, "xs1": xs1, "xs2": xs2}})
    
    return {
        "status": 400,
        "message": "Instruction decoded"
        }
