use serde::{Deserialize, Serialize};

/// 分页请求参数
#[derive(Debug, Clone, Deserialize)]
pub struct PageRequest {
    /// 页码，从 1 开始
    #[serde(default = "default_page")]
    pub page: u32,
    /// 每页条数
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    /// 排序字段
    #[serde(default)]
    pub sort_by: Option<String>,
    /// 排序方向：asc / desc
    #[serde(default)]
    pub sort_order: Option<SortOrder>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    Desc,
}

/// 分页响应包装
#[derive(Debug, Clone, Serialize)]
pub struct PageResponse<T: Serialize> {
    /// 数据列表
    pub items: Vec<T>,
    /// 总记录数
    pub total: i64,
    /// 当前页码
    pub page: u32,
    /// 每页条数
    pub page_size: u32,
    /// 总页数
    pub total_pages: u32,
}

impl PageRequest {
    /// 计算 SQL OFFSET
    pub fn offset(&self) -> i64 {
        ((self.page.saturating_sub(1)) * self.page_size) as i64
    }

    /// 获取 LIMIT 值（限制最大 100）
    pub fn limit(&self) -> i64 {
        self.page_size.min(100) as i64
    }
}

impl<T: Serialize> PageResponse<T> {
    pub fn new(items: Vec<T>, total: i64, page: u32, page_size: u32) -> Self {
        let total_pages = if page_size > 0 {
            ((total as u32) + page_size - 1) / page_size
        } else {
            0
        };
        Self {
            items,
            total,
            page,
            page_size,
            total_pages,
        }
    }
}

fn default_page() -> u32 {
    1
}

fn default_page_size() -> u32 {
    20
}
