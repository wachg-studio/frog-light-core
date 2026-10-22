use crate::model::Rule;
use std::collections::HashMap;

pub const OS_NAME: &str = if cfg!(target_os = "windows") {
    "windows"
} else if cfg!(target_os = "macos") {
    "osx"
} else {
    "linux"
};

pub const ARCH_NAME: &str = if cfg!(target_arch = "x86") {
    "x86"
} else if cfg!(target_arch = "aarch64") {
    "arm64"
} else {
    "x86_64"
};

fn matches(rule: &Rule, features: &HashMap<&str, bool>) -> bool {
    if let Some(os) = &rule.os {
        if os.name.as_deref().is_some_and(|n| n != OS_NAME) {
            return false;
        }
        if os.arch.as_deref().is_some_and(|a| a != ARCH_NAME) {
            return false;
        }
    }
    if let Some(req) = &rule.features {
        for (k, v) in req {
            if features.get(k.as_str()).copied().unwrap_or(false) != *v {
                return false;
            }
        }
    }
    true
}

/// Mojang 规则语义：默认拒绝，按顺序最后一条命中的规则决定结果。
pub fn allowed(rules: &[Rule], features: &HashMap<&str, bool>) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut ok = false;
    for r in rules {
        if matches(r, features) {
            ok = r.action == "allow";
        }
    }
    ok
}
