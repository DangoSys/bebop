/// 内存映射模块
/// 
/// 负责管理 BEMU 的内存地址空间映射
/// 支持：
/// - 物理内存到虚拟内存的映射
/// - 内存区域管理
/// - 高效的内存访问

use log::{debug, info, warn};
use std::collections::HashMap;

/// 内存页大小（4KB）
pub const PAGE_SIZE: usize = 4096;

/// 内存区域权限
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPermission {
    /// 只读
    ReadOnly,
    /// 只写
    WriteOnly,
    /// 读写
    ReadWrite,
    /// 无权限
    None,
}

/// 内存区域描述
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    /// 区域名称
    pub name: String,
    /// 起始地址（物理地址）
    pub phys_start: u64,
    /// 起始地址（虚拟地址）
    pub virt_start: u64,
    /// 区域大小（字节）
    pub size: usize,
    /// 权限
    pub permission: MemoryPermission,
    /// 是否已映射
    pub mapped: bool,
}

impl MemoryRegion {
    /// 创建新的内存区域
    pub fn new(
        name: &str,
        phys_start: u64,
        virt_start: u64,
        size: usize,
        permission: MemoryPermission,
    ) -> Self {
        Self {
            name: name.to_string(),
            phys_start,
            virt_start,
            size,
            permission,
            mapped: false,
        }
    }
    
    /// 检查地址是否在区域内（物理地址）
    pub fn contains_phys(&self, addr: u64) -> bool {
        addr >= self.phys_start && addr < self.phys_start + self.size as u64
    }
    
    /// 检查地址是否在区域内（虚拟地址）
    pub fn contains_virt(&self, addr: u64) -> bool {
        addr >= self.virt_start && addr < self.virt_start + self.size as u64
    }
    
    /// 物理地址到虚拟地址的转换
    pub fn phys_to_virt(&self, phys_addr: u64) -> Option<u64> {
        if self.contains_phys(phys_addr) {
            Some(self.virt_start + (phys_addr - self.phys_start))
        } else {
            None
        }
    }
    
    /// 虚拟地址到物理地址的转换
    pub fn virt_to_phys(&self, virt_addr: u64) -> Option<u64> {
        if self.contains_virt(virt_addr) {
            Some(self.phys_start + (virt_addr - self.virt_start))
        } else {
            None
        }
    }
}

/// 内存映射器
pub struct MemoryMapper {
    /// 内存区域列表
    regions: HashMap<String, MemoryRegion>,
    /// 物理地址到虚拟地址的映射缓存（用于性能优化）
    phys_to_virt_cache: HashMap<u64, u64>,
    /// 虚拟地址到物理地址的映射缓存
    virt_to_phys_cache: HashMap<u64, u64>,
    /// 是否启用缓存
    cache_enabled: bool,
    /// 是否启用详细日志
    verbose: bool,
}

impl MemoryMapper {
    /// 创建新的内存映射器
    pub fn new() -> Self {
        Self {
            regions: HashMap::new(),
            phys_to_virt_cache: HashMap::new(),
            virt_to_phys_cache: HashMap::new(),
            cache_enabled: true,
            verbose: false,
        }
    }
    
    /// 创建带详细日志的映射器
    pub fn with_verbose(verbose: bool) -> Self {
        Self {
            regions: HashMap::new(),
            phys_to_virt_cache: HashMap::new(),
            virt_to_phys_cache: HashMap::new(),
            cache_enabled: true,
            verbose,
        }
    }
    
    /// 注册内存区域
    /// 
    /// # Arguments
    /// * `region` - 内存区域描述
    /// 
    /// # Returns
    /// * `Result<(), String>` - 成功或错误信息
    pub fn register_region(&mut self, region: MemoryRegion) -> Result<(), String> {
        if self.regions.contains_key(&region.name) {
            return Err(format!("Memory region '{}' already exists", region.name));
        }
        
        info!(
            "Registering memory region: {} (phys: 0x{:x}, virt: 0x{:x}, size: {}KB)",
            region.name,
            region.phys_start,
            region.virt_start,
            region.size / 1024
        );
        
        self.regions.insert(region.name.clone(), region);
        Ok(())
    }
    
    /// 创建并注册一个内存区域
    pub fn create_region(
        &mut self,
        name: &str,
        phys_start: u64,
        virt_start: u64,
        size: usize,
        permission: MemoryPermission,
    ) -> Result<(), String> {
        let region = MemoryRegion::new(name, phys_start, virt_start, size, permission);
        self.register_region(region)
    }
    
    /// 映射内存区域
    pub fn map_region(&mut self, name: &str) -> Result<(), String> {
        let region = self.regions.get_mut(name)
            .ok_or_else(|| format!("Memory region '{}' not found", name))?;
        
        if region.mapped {
            warn!("Memory region '{}' is already mapped", name);
            return Ok(());
        }
        
        info!("Mapping memory region: {}", name);
        region.mapped = true;
        
        // 清除缓存
        if self.cache_enabled {
            self.clear_cache();
        }
        
        Ok(())
    }
    
    /// 取消映射内存区域
    pub fn unmap_region(&mut self, name: &str) -> Result<(), String> {
        let region = self.regions.get_mut(name)
            .ok_or_else(|| format!("Memory region '{}' not found", name))?;
        
        if !region.mapped {
            warn!("Memory region '{}' is not mapped", name);
            return Ok(());
        }
        
        info!("Unmapping memory region: {}", name);
        region.mapped = false;
        
        // 清除缓存
        if self.cache_enabled {
            self.clear_cache();
        }
        
        Ok(())
    }
    
    /// 物理地址到虚拟地址的转换
    /// 
    /// # Arguments
    /// * `phys_addr` - 物理地址
    /// 
    /// # Returns
    /// * `Option<u64>` - 虚拟地址，如果转换失败则返回 None
    pub fn phys_to_virt(&mut self, phys_addr: u64) -> Option<u64> {
        // 尝试从缓存获取
        if self.cache_enabled {
            if let Some(&virt_addr) = self.phys_to_virt_cache.get(&phys_addr) {
                if self.verbose {
                    debug!("Cache hit: phys 0x{:x} -> virt 0x{:x}", phys_addr, virt_addr);
                }
                return Some(virt_addr);
            }
        }
        
        // 遍历所有区域查找
        for region in self.regions.values() {
            if !region.mapped {
                continue;
            }
            
            if let Some(virt_addr) = region.phys_to_virt(phys_addr) {
                // 更新缓存
                if self.cache_enabled {
                    self.phys_to_virt_cache.insert(phys_addr, virt_addr);
                    self.virt_to_phys_cache.insert(virt_addr, phys_addr);
                }
                
                if self.verbose {
                    debug!(
                        "Translated: phys 0x{:x} -> virt 0x{:x} (region: {})",
                        phys_addr, virt_addr, region.name
                    );
                }
                
                return Some(virt_addr);
            }
        }
        
        if self.verbose {
            debug!("Failed to translate phys address 0x{:x}", phys_addr);
        }
        
        None
    }
    
    /// 虚拟地址到物理地址的转换
    /// 
    /// # Arguments
    /// * `virt_addr` - 虚拟地址
    /// 
    /// # Returns
    /// * `Option<u64>` - 物理地址，如果转换失败则返回 None
    pub fn virt_to_phys(&mut self, virt_addr: u64) -> Option<u64> {
        // 尝试从缓存获取
        if self.cache_enabled {
            if let Some(&phys_addr) = self.virt_to_phys_cache.get(&virt_addr) {
                if self.verbose {
                    debug!("Cache hit: virt 0x{:x} -> phys 0x{:x}", virt_addr, phys_addr);
                }
                return Some(phys_addr);
            }
        }
        
        // 遍历所有区域查找
        for region in self.regions.values() {
            if !region.mapped {
                continue;
            }
            
            if let Some(phys_addr) = region.virt_to_phys(virt_addr) {
                // 更新缓存
                if self.cache_enabled {
                    self.phys_to_virt_cache.insert(phys_addr, virt_addr);
                    self.virt_to_phys_cache.insert(virt_addr, phys_addr);
                }
                
                if self.verbose {
                    debug!(
                        "Translated: virt 0x{:x} -> phys 0x{:x} (region: {})",
                        virt_addr, phys_addr, region.name
                    );
                }
                
                return Some(phys_addr);
            }
        }
        
        if self.verbose {
            debug!("Failed to translate virt address 0x{:x}", virt_addr);
        }
        
        None
    }
    
    /// 检查物理地址是否可访问
    pub fn is_phys_accessible(&self, phys_addr: u64, permission: MemoryPermission) -> bool {
        for region in self.regions.values() {
            if !region.mapped {
                continue;
            }
            
            if region.contains_phys(phys_addr) {
                return self.check_permission(region.permission, permission);
            }
        }
        false
    }
    
    /// 检查虚拟地址是否可访问
    pub fn is_virt_accessible(&self, virt_addr: u64, permission: MemoryPermission) -> bool {
        for region in self.regions.values() {
            if !region.mapped {
                continue;
            }
            
            if region.contains_virt(virt_addr) {
                return self.check_permission(region.permission, permission);
            }
        }
        false
    }
    
    /// 清除映射缓存
    pub fn clear_cache(&mut self) {
        if self.verbose {
            debug!("Clearing address translation cache");
        }
        self.phys_to_virt_cache.clear();
        self.virt_to_phys_cache.clear();
    }
    
    /// 启用或禁用缓存
    pub fn set_cache_enabled(&mut self, enabled: bool) {
        self.cache_enabled = enabled;
        if !enabled {
            self.clear_cache();
        }
    }
    
    /// 获取所有已注册的内存区域信息
    pub fn get_regions_info(&self) -> Vec<&MemoryRegion> {
        self.regions.values().collect()
    }
    
    /// 打印内存映射信息
    pub fn print_memory_map(&self) {
        info!("Memory Map:");
        info!("  {:<20} {:<18} {:<18} {:<10} {:<8}", 
              "Region", "Phys Start", "Virt Start", "Size", "Mapped");
        info!("  {}", "-".repeat(80));
        
        for region in self.regions.values() {
            info!(
                "  {:<20} 0x{:016x} 0x{:016x} {:<10} {}",
                region.name,
                region.phys_start,
                region.virt_start,
                format!("{}KB", region.size / 1024),
                if region.mapped { "Yes" } else { "No" }
            );
        }
    }
    
    /// 检查权限是否兼容
    fn check_permission(&self, region_perm: MemoryPermission, required_perm: MemoryPermission) -> bool {
        match (region_perm, required_perm) {
            (MemoryPermission::None, _) => false,
            (_, MemoryPermission::None) => true,
            (MemoryPermission::ReadOnly, MemoryPermission::ReadOnly) => true,
            (MemoryPermission::WriteOnly, MemoryPermission::WriteOnly) => true,
            (MemoryPermission::ReadWrite, _) => true,
            _ => false,
        }
    }
}

impl Default for MemoryMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_region_creation() {
        let region = MemoryRegion::new(
            "test_region",
            0x1000,
            0x80000000,
            4096,
            MemoryPermission::ReadWrite,
        );
        
        assert_eq!(region.name, "test_region");
        assert_eq!(region.phys_start, 0x1000);
        assert_eq!(region.virt_start, 0x80000000);
        assert_eq!(region.size, 4096);
        assert_eq!(region.permission, MemoryPermission::ReadWrite);
        assert!(!region.mapped);
    }
    
    #[test]
    fn test_memory_region_translation() {
        let region = MemoryRegion::new(
            "test_region",
            0x1000,
            0x80000000,
            4096,
            MemoryPermission::ReadWrite,
        );
        
        // 测试物理地址到虚拟地址
        assert_eq!(region.phys_to_virt(0x1000), Some(0x80000000));
        assert_eq!(region.phys_to_virt(0x1100), Some(0x80000100));
        assert_eq!(region.phys_to_virt(0x2000), None); // 超出范围
        
        // 测试虚拟地址到物理地址
        assert_eq!(region.virt_to_phys(0x80000000), Some(0x1000));
        assert_eq!(region.virt_to_phys(0x80000100), Some(0x1100));
        assert_eq!(region.virt_to_phys(0x90000000), None); // 超出范围
    }
    
    #[test]
    fn test_memory_mapper_registration() {
        let mut mapper = MemoryMapper::new();
        
        // 注册区域
        assert!(mapper.create_region(
            "main_memory",
            0x0,
            0x80000000,
            1024 * 1024, // 1MB
            MemoryPermission::ReadWrite,
        ).is_ok());
        
        // 重复注册应该失败
        assert!(mapper.create_region(
            "main_memory",
            0x0,
            0x80000000,
            1024 * 1024,
            MemoryPermission::ReadWrite,
        ).is_err());
    }
    
    #[test]
    fn test_memory_mapper_translation() {
        let mut mapper = MemoryMapper::new();
        
        mapper.create_region(
            "main_memory",
            0x0,
            0x80000000,
            1024 * 1024,
            MemoryPermission::ReadWrite,
        ).unwrap();
        
        mapper.map_region("main_memory").unwrap();
        
        // 测试地址转换
        assert_eq!(mapper.phys_to_virt(0x100), Some(0x80000100));
        assert_eq!(mapper.virt_to_phys(0x80000100), Some(0x100));
        
        // 测试未映射区域的地址转换
        mapper.unmap_region("main_memory").unwrap();
        assert_eq!(mapper.phys_to_virt(0x100), None);
    }
    
    #[test]
    fn test_memory_mapper_cache() {
        let mut mapper = MemoryMapper::with_verbose(true);
        
        mapper.create_region(
            "cached_region",
            0x0,
            0x80000000,
            4096,
            MemoryPermission::ReadWrite,
        ).unwrap();
        
        mapper.map_region("cached_region").unwrap();
        
        // 第一次访问（缓存未命中）
        assert_eq!(mapper.phys_to_virt(0x100), Some(0x80000100));
        
        // 第二次访问（应该命中缓存）
        assert_eq!(mapper.phys_to_virt(0x100), Some(0x80000100));
        
        // 清除缓存后再次访问
        mapper.clear_cache();
        assert_eq!(mapper.phys_to_virt(0x100), Some(0x80000100));
    }
}
