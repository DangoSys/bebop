import asyncio

config = {
    "type": "event",
    "name": "reservation station",
    "description": "handle RS allocation and dispatch",
    "subscribes": ["frontend.rs.dispatch"],
    "emits": [],
    "flows": ["mvin"],
}


async def handler(data, context):
    funct, xs1, xs2 = data.get("funct"), data.get("xs1"), data.get("xs2")
    rob_id = data.get("rob_id")
    domain_id = data.get("domain_id")

    print(f"RS dispatching entry: domain_id={domain_id}, rob_id={rob_id}, funct={funct}, xs1={xs1:#x}, xs2={xs2:#x}")
    print(f"RS entry {domain_id} ready for dispatch")

    return {"message": "RS entry processed"}
