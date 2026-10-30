#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Workbuddy,
    Trae,
    Qoder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Cn,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scope {
    pub platform: Platform,
    pub region: Region,
}

impl Scope {
    pub fn parse(value: &str) -> Result<Self, String> {
        let (platform, region) = value
            .split_once('-')
            .ok_or_else(|| format!("未知作用域：{value}"))?;
        let platform = match platform {
            "workbuddy" => Platform::Workbuddy,
            "trae" => Platform::Trae,
            "qoder" => Platform::Qoder,
            _ => return Err(format!("未知平台：{platform}")),
        };
        let region = match region {
            "cn" => Region::Cn,
            "global" => Region::Global,
            _ => return Err(format!("未知区域：{region}")),
        };
        Ok(Self { platform, region })
    }

    pub fn as_str(self) -> &'static str {
        match (self.platform, self.region) {
            (Platform::Workbuddy, Region::Cn) => "workbuddy-cn",
            (Platform::Workbuddy, Region::Global) => "workbuddy-global",
            (Platform::Trae, Region::Cn) => "trae-cn",
            (Platform::Trae, Region::Global) => "trae-global",
            (Platform::Qoder, Region::Cn) => "qoder-cn",
            (Platform::Qoder, Region::Global) => "qoder-global",
        }
    }

    pub fn billing_base(self) -> &'static str {
        match self.region {
            Region::Cn => "https://www.codebuddy.cn",
            Region::Global => "https://www.workbuddy.ai",
        }
    }
}
