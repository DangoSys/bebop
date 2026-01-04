import asyncio

config = {
    "type": "event",
    "name": "decoder",
    "description": "clean build directory",
    "subscribes": ["frontend.decode"],
    "emits": ["frontend.rob.allocate"],
    "flows": ["mvin"],
}


async def handler(data, context):
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
