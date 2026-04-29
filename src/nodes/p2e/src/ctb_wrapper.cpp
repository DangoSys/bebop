#include <string>
#include "ICtb.h"

// C wrapper functions for Rust FFI
extern "C" {

// Wrapper for ctb::ctbMgr::init that accepts C strings and converts to std::string
bool ctb_init_wrapper(
    void* mgr,
    const char* fpga_id,
    const char* case_home,
    const char* rtcfg_path
) {
    if (!mgr || !fpga_id || !case_home || !rtcfg_path) {
        return false;
    }

    // Cast to vvac::ICtbMgr and call init with std::string
    vvac::ICtbMgr* ctb_mgr = static_cast<vvac::ICtbMgr*>(mgr);
    return ctb_mgr->init(
        std::string(fpga_id),
        std::string(case_home),
        std::string(rtcfg_path)
    );
}

// Wrapper for vvac::CtbBuilder::create()
void* ctb_builder_create_wrapper() {
    return vvac::CtbBuilder::create();
}

} // extern "C"
