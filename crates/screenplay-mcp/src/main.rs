//! stdio 模式入口。实现全部在 lib 里，二进制保持与之前完全一致的行为。

fn main() -> Result<(), Box<dyn std::error::Error>> {
    screenplay_mcp::cli_main()
}
