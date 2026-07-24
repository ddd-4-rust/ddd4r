//! External-only profiler toolchain metadata.

/// External diagnostic tool category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalToolKind {
    /// Sampling CPU profiler.
    SamplingProfiler,
    /// Kernel and userspace performance counters.
    SystemProfiler,
    /// Dynamic eBPF tracing.
    DynamicTracing,
    /// Native debugger.
    Debugger,
}

/// One tool intentionally not linked into ddd4r applications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalDiagnosticTool {
    /// Command name.
    pub name: &'static str,
    /// Tool category.
    pub kind: ExternalToolKind,
    /// Required build/runtime preparation.
    pub prerequisite: &'static str,
    /// Always false: these tools remain outside the application dependency graph.
    pub embedded: bool,
}

/// Authoritative external-tool inventory.
pub struct ExternalToolchain;

impl ExternalToolchain {
    /// Returns samply, perf, eBPF/bpftrace, and lldb as external-only tools.
    pub const fn all() -> &'static [ExternalDiagnosticTool] {
        &[
            ExternalDiagnosticTool {
                name: "samply",
                kind: ExternalToolKind::SamplingProfiler,
                prerequisite: "profiling binary and matching symbols",
                embedded: false,
            },
            ExternalDiagnosticTool {
                name: "perf",
                kind: ExternalToolKind::SystemProfiler,
                prerequisite: "Linux perf permissions, frame pointers, and symbols",
                embedded: false,
            },
            ExternalDiagnosticTool {
                name: "bpftrace",
                kind: ExternalToolKind::DynamicTracing,
                prerequisite: "Linux eBPF permissions and stable native symbols",
                embedded: false,
            },
            ExternalDiagnosticTool {
                name: "lldb",
                kind: ExternalToolKind::Debugger,
                prerequisite: "debugger attach permission and matching symbols",
                embedded: false,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expert_tools_are_never_embedded() {
        let tools = ExternalToolchain::all();
        assert_eq!(tools.len(), 4);
        assert!(tools.iter().all(|tool| !tool.embedded));
        assert_eq!(
            tools.iter().map(|tool| tool.name).collect::<Vec<_>>(),
            ["samply", "perf", "bpftrace", "lldb"]
        );
    }
}
