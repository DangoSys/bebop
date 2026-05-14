#include <cstdint>
#include <string>
#include <iostream>
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
    std::cout << "[ctb_wrapper] ctb_init_wrapper called" << std::endl;
    std::cout << "[ctb_wrapper] mgr: " << mgr << std::endl;
    std::cout << "[ctb_wrapper] fpga_id: " << (fpga_id ? fpga_id : "NULL") << std::endl;
    std::cout << "[ctb_wrapper] case_home: " << (case_home ? case_home : "NULL") << std::endl;
    std::cout << "[ctb_wrapper] rtcfg_path: " << (rtcfg_path ? rtcfg_path : "NULL") << std::endl;

    if (!mgr || !fpga_id || !case_home || !rtcfg_path) {
        std::cout << "[ctb_wrapper] NULL parameter detected, returning false" << std::endl;
        return false;
    }

    std::cout << "[ctb_wrapper] Casting to vvac::ICtbMgr*" << std::endl;
    // Cast to vvac::ICtbMgr and call init with std::string
    vvac::ICtbMgr* ctb_mgr = static_cast<vvac::ICtbMgr*>(mgr);
    std::cout << "[ctb_wrapper] ctb_mgr: " << ctb_mgr << std::endl;

    std::cout << "[ctb_wrapper] Calling ctb_mgr->init()" << std::endl;
    bool result = ctb_mgr->init(
        std::string(fpga_id),
        std::string(case_home),
        std::string(rtcfg_path)
    );
    std::cout << "[ctb_wrapper] ctb_mgr->init() returned: " << result << std::endl;

    return result;
}

// Wrapper for vvac::CtbBuilder::create()
void* ctb_builder_create_wrapper() {
    std::cout << "[ctb_wrapper] ctb_builder_create_wrapper called" << std::endl;

    // CRITICAL: Set environment variables BEFORE calling create()
    // These must be set in C++ land, not Rust, because the library reads them during initialization
    setenv("VMRI_LOG_LEVEL", "0", 1);
    setenv("VVAC_LOG_LEVEL", "0", 1);
    setenv("RBMGR_LOG_LEVEL", "0", 1);
    setenv("RBMGR_DUMP_DATA", "1", 1);
    setenv("RTL_DBG_SIZE", "128", 1);
    // Onboard mode
    setenv("VMRI_WORK_MODE", "3", 1);  
    // Onboard mode
    setenv("VVAC_WORK_MODE", "0", 1);  

    std::cout << "[ctb_wrapper] Environment variables set" << std::endl;

    void* result = vvac::CtbBuilder::create();
    std::cout << "[ctb_wrapper] vvac::CtbBuilder::create() returned: " << result << std::endl;
    return result;
}

// Wrapper for vvac::ICtbMgr::quit()
void ctb_quit_wrapper(void* mgr) {
    std::cout << "[ctb_wrapper] ctb_quit_wrapper called" << std::endl;
    if (!mgr) {
        std::cout << "[ctb_wrapper] NULL mgr pointer" << std::endl;
        return;
    }
    vvac::ICtbMgr* ctb_mgr = static_cast<vvac::ICtbMgr*>(mgr);
    ctb_mgr->quit();
    std::cout << "[ctb_wrapper] quit() completed" << std::endl;
}

} // extern "C"
