//! Universal Hardware Graph, Endpoints, Clock Domains, Device State Machines, and Route Planner.
//! Implements Sections 2, 3, 6, 7, 8, and 12 of the HWCode Specification.

use std::collections::HashMap;
use std::fmt;

/// An addressable endpoint in the Hardware Graph (e.g., `/F3`, `/F3::dma`, `/N0/F3::memory`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Endpoint {
    pub node_cluster: Option<String>, // e.g., Some("N0") for /N0/F3
    pub resource_id: String,          // e.g., "F3"
    pub sub_resource: Option<String>, // e.g., Some("dma"), Some("memory"), Some("register")
}

impl Endpoint {
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim().trim_start_matches('/');
        let (path_part, sub_part) = if let Some((left, right)) = trimmed.split_once("::") {
            (left, Some(right.to_string()))
        } else {
            (trimmed, None)
        };

        if let Some((cluster, res)) = path_part.split_once('/') {
            Self {
                node_cluster: Some(cluster.to_string()),
                resource_id: res.to_string(),
                sub_resource: sub_part,
            }
        } else {
            Self {
                node_cluster: None,
                resource_id: path_part.to_string(),
                sub_resource: sub_part,
            }
        }
    }

    pub fn base_id(&self) -> String {
        if let Some(cluster) = &self.node_cluster {
            format!("/{}/{}", cluster, self.resource_id)
        } else {
            format!("/{}", self.resource_id)
        }
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let base = self.base_id();
        if let Some(sub) = &self.sub_resource {
            write!(f, "{}::{}", base, sub)
        } else {
            write!(f, "{}", base)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceClass {
    CPU,
    RAM,
    GPU,
    Storage,
    Network,
    FPGA,
    NPU,
    Sensor,
    Display,
    Custom(String),
}

impl DeviceClass {
    pub fn from_name(name: &str) -> Self {
        match name.to_uppercase().as_str() {
            "CPU" => Self::CPU,
            "RAM" | "SYSTEMRAM" | "MEMORY" => Self::RAM,
            "GPU" => Self::GPU,
            "STORAGE" | "NVME" | "DISK" => Self::Storage,
            "NETWORK" | "NIC" => Self::Network,
            "FPGA" => Self::FPGA,
            "NPU" => Self::NPU,
            "SENSOR" => Self::Sensor,
            "DISPLAY" => Self::Display,
            other => Self::Custom(other.to_string()),
        }
    }
}

impl fmt::Display for DeviceClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceClass::Custom(s) => write!(f, "{}", s),
            other => write!(f, "{:?}", other),
        }
    }
}

/// Clock domain relationship (Section 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClockRelation {
    Synchronous,
    Derived,
    Mesochronous,
    Asynchronous,
}

impl fmt::Display for ClockRelation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone)]
pub struct ClockDomain {
    pub name: String,
    pub frequency_mhz: u64,
}

/// Device state machine transition rule (Section 7).
#[derive(Debug, Clone)]
pub struct StateTransitionRule {
    pub from_state: String,
    pub to_state: String,
    pub trigger: String, // e.g., "REG.START = 1" or "reset()"
}

/// Device state machine specification (Section 7 & 12).
#[derive(Debug, Clone)]
pub struct DeviceStateMachine {
    pub device_name: String,
    pub states: Vec<String>,
    pub initial_state: String,
    pub transitions: Vec<StateTransitionRule>,
    pub operation_requirements: HashMap<String, String>, // op_name -> required_state
    pub registers: Vec<(String, String)>,                // (reg_name, reg_type)
}

/// A Node in the Universal Hardware Graph (Section 3).
#[derive(Debug, Clone)]
pub struct HardwareNode {
    pub endpoint_id: String, // e.g., "/F1", "/F3", "/N0/F3"
    pub name: String,
    pub class: DeviceClass,
    pub memory_domain: String,
    pub preferred_layout: String,
    pub supported_layouts: Vec<String>,
    pub clock_domain: ClockDomain,
    pub sub_resources: Vec<String>,
    pub capabilities: Vec<String>,
    pub coherent_with_cpu: bool,
    pub idle_power_watts: f64,
    pub max_power_watts: f64,
    pub thermal_limit_c: f64,
    pub state_machine: Option<DeviceStateMachine>,
}

/// An Edge connecting two hardware resources in the Hardware Graph (Section 3).
#[derive(Debug, Clone)]
pub struct HardwareEdge {
    pub from: String,
    pub to: String,
    pub protocol: String,
    pub bandwidth_gbps: f64,
    pub latency_ns: u64,
    pub dma_available: bool,
    pub dma_engine: Option<String>,
    pub coherent: bool,
    pub addressable: bool,
    pub power_mw_per_gb: f64,
    pub clock_relation: ClockRelation,
    pub required_capability: Option<String>,
    pub online: bool,
}

/// Result of planning a route through the Universal Hardware Graph (Sections 3, 8, 11).
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedRoute {
    pub source: String,
    pub destination: String,
    pub hops: Vec<String>,
    pub protocols: Vec<String>,
    pub dma_engines: Vec<String>,
    pub effective_bandwidth_gbps: f64,
    pub total_latency_ns: u64,
    pub estimated_power_watts: f64,
    pub requires_memory_barrier: bool,
    pub crosses_async_clock: bool,
    pub fallback_hops: Option<Vec<String>>,
    pub rejected_alternatives: Vec<(Vec<String>, String)>, // (path, rejection_reason)
    pub bottleneck_link: String,
}

impl PlannedRoute {
    pub fn format_path(&self) -> String {
        self.hops.join(" -> ")
    }
}

/// The Universal Hardware Graph (Section 3).
#[derive(Debug, Clone)]
pub struct HardwareGraph {
    pub nodes: HashMap<String, HardwareNode>,
    pub node_order: Vec<String>,
    pub edges: Vec<HardwareEdge>,
    pub granted_capabilities: Vec<String>,
}

impl Default for HardwareGraph {
    fn default() -> Self {
        Self::standard_machine()
    }
}

impl HardwareGraph {
    /// Constructs a canonical workstation/embedded/cluster Hardware Graph matching all PDF examples.
    pub fn standard_machine() -> Self {
        let mut graph = Self {
            nodes: HashMap::new(),
            node_order: Vec::new(),
            edges: Vec::new(),
            granted_capabilities: vec![
                "Cap::Compute".to_string(),
                "Cap::DMA".to_string(),
                "Cap::QueueSubmit".to_string(),
                "Cap::PeerMemory".to_string(),
                "Cap::StorageRead".to_string(),
                "Cap::SensorAccess".to_string(),
                "Cap::NetworkRDMA".to_string(),
            ],
        };

        // /F1: CPU Core Complex
        graph.add_node(HardwareNode {
            endpoint_id: "/F1".to_string(),
            name: "Host CPU Complex (x86_64 / ARM64)".to_string(),
            class: DeviceClass::CPU,
            memory_domain: "SystemRAM".to_string(),
            preferred_layout: "Linear".to_string(),
            supported_layouts: vec![
                "Linear".to_string(),
                "RowMajor".to_string(),
                "ColMajor".to_string(),
                "Packed".to_string(),
            ],
            clock_domain: ClockDomain {
                name: "CPU_CLK".to_string(),
                frequency_mhz: 3600,
            },
            sub_resources: vec![
                "core".to_string(),
                "cache".to_string(),
                "interrupt".to_string(),
                "register".to_string(),
            ],
            capabilities: vec!["Cap::Compute".to_string(), "Cap::Control".to_string()],
            coherent_with_cpu: true,
            idle_power_watts: 10.0,
            max_power_watts: 65.0,
            thermal_limit_c: 95.0,
            state_machine: None,
        });

        // /F2: System RAM
        graph.add_node(HardwareNode {
            endpoint_id: "/F2".to_string(),
            name: "System DDR5 RAM Controller".to_string(),
            class: DeviceClass::RAM,
            memory_domain: "SystemRAM".to_string(),
            preferred_layout: "Linear".to_string(),
            supported_layouts: vec![
                "Linear".to_string(),
                "RowMajor".to_string(),
                "ColMajor".to_string(),
                "Tiled2D".to_string(),
            ],
            clock_domain: ClockDomain {
                name: "CPU_CLK".to_string(),
                frequency_mhz: 3600,
            },
            sub_resources: vec!["memory".to_string(), "dma".to_string()],
            capabilities: vec!["Cap::DMA".to_string(), "Cap::SharedMemory".to_string()],
            coherent_with_cpu: true,
            idle_power_watts: 3.0,
            max_power_watts: 15.0,
            thermal_limit_c: 85.0,
            state_machine: None,
        });

        // /F3: Discrete GPU Accelerator
        let mut gpu_ops = HashMap::new();
        gpu_ops.insert("dispatch_kernel".to_string(), "Active".to_string());
        graph.add_node(HardwareNode {
            endpoint_id: "/F3".to_string(),
            name: "Tensor/Compute GPU Accelerator".to_string(),
            class: DeviceClass::GPU,
            memory_domain: "VRAM".to_string(),
            preferred_layout: "Tiled2D".to_string(),
            supported_layouts: vec![
                "Tiled2D".to_string(),
                "Swizzled".to_string(),
                "Linear".to_string(),
            ],
            clock_domain: ClockDomain {
                name: "GPU_CLK".to_string(),
                frequency_mhz: 1800,
            },
            sub_resources: vec![
                "memory".to_string(),
                "dma".to_string(),
                "queue".to_string(),
                "register".to_string(),
                "interrupt".to_string(),
            ],
            capabilities: vec![
                "Cap::Compute".to_string(),
                "Cap::DMA".to_string(),
                "Cap::QueueSubmit".to_string(),
                "Cap::PeerMemory".to_string(),
            ],
            coherent_with_cpu: false,
            idle_power_watts: 15.0,
            max_power_watts: 250.0,
            thermal_limit_c: 88.0,
            state_machine: Some(DeviceStateMachine {
                device_name: "GPU".to_string(),
                states: vec![
                    "Idle".to_string(),
                    "Active".to_string(),
                    "Error".to_string(),
                    "Lost".to_string(),
                ],
                initial_state: "Active".to_string(),
                transitions: vec![StateTransitionRule {
                    from_state: "Idle".to_string(),
                    to_state: "Active".to_string(),
                    trigger: "REG.ENABLE = 1".to_string(),
                }],
                operation_requirements: gpu_ops,
                registers: vec![
                    ("ENABLE".to_string(), "u32".to_string()),
                    ("STATUS".to_string(), "u32".to_string()),
                ],
            }),
        });

        // /F4: NVMe High-Speed Storage
        graph.add_node(HardwareNode {
            endpoint_id: "/F4".to_string(),
            name: "NVMe PCIe Gen4 Solid-State Storage".to_string(),
            class: DeviceClass::Storage,
            memory_domain: "PersistentMemory".to_string(),
            preferred_layout: "Linear".to_string(),
            supported_layouts: vec!["Linear".to_string(), "Packed".to_string()],
            clock_domain: ClockDomain {
                name: "PCIE_CLK".to_string(),
                frequency_mhz: 1000,
            },
            sub_resources: vec![
                "memory".to_string(),
                "dma".to_string(),
                "queue".to_string(),
                "interrupt".to_string(),
            ],
            capabilities: vec!["Cap::DMA".to_string(), "Cap::StorageRead".to_string()],
            coherent_with_cpu: false,
            idle_power_watts: 1.5,
            max_power_watts: 9.0,
            thermal_limit_c: 75.0,
            state_machine: None,
        });

        // /F5: Network Interface Card (NIC)
        graph.add_node(HardwareNode {
            endpoint_id: "/F5".to_string(),
            name: "RDMA 100GbE Network Interface".to_string(),
            class: DeviceClass::Network,
            memory_domain: "DeviceRAM".to_string(),
            preferred_layout: "Linear".to_string(),
            supported_layouts: vec!["Linear".to_string(), "Packed".to_string()],
            clock_domain: ClockDomain {
                name: "NET_CLK".to_string(),
                frequency_mhz: 800,
            },
            sub_resources: vec![
                "queue".to_string(),
                "dma".to_string(),
                "interrupt".to_string(),
                "offload".to_string(),
            ],
            capabilities: vec!["Cap::DMA".to_string(), "Cap::NetworkRDMA".to_string()],
            coherent_with_cpu: false,
            idle_power_watts: 4.0,
            max_power_watts: 25.0,
            thermal_limit_c: 85.0,
            state_machine: None,
        });

        // /F6: FPGA Programmable Logic Block
        graph.add_node(HardwareNode {
            endpoint_id: "/F6".to_string(),
            name: "FPGA Reconfigurable Fabric".to_string(),
            class: DeviceClass::FPGA,
            memory_domain: "LocalMemory".to_string(),
            preferred_layout: "Packed".to_string(),
            supported_layouts: vec![
                "Packed".to_string(),
                "Linear".to_string(),
                "Tiled2D".to_string(),
            ],
            clock_domain: ClockDomain {
                name: "FPGA_CLK".to_string(),
                frequency_mhz: 250,
            },
            sub_resources: vec![
                "memory".to_string(),
                "dma".to_string(),
                "register".to_string(),
                "interrupt".to_string(),
            ],
            capabilities: vec!["Cap::DMA".to_string(), "Cap::FPGAStream".to_string()],
            coherent_with_cpu: false,
            idle_power_watts: 5.0,
            max_power_watts: 40.0,
            thermal_limit_c: 85.0,
            state_machine: None,
        });

        // /F7: I2C Thermal & Telemetry Sensor (Section 7 & 12 example)
        let mut sensor_ops = HashMap::new();
        sensor_ops.insert("read_temperature".to_string(), "Active".to_string());
        sensor_ops.insert("read_telemetry".to_string(), "Active".to_string());
        sensor_ops.insert("calibrate".to_string(), "Configuring".to_string());

        graph.add_node(HardwareNode {
            endpoint_id: "/F7".to_string(),
            name: "Precision I2C Thermal Sensor".to_string(),
            class: DeviceClass::Sensor,
            memory_domain: "MMIO".to_string(),
            preferred_layout: "Linear".to_string(),
            supported_layouts: vec!["Linear".to_string()],
            clock_domain: ClockDomain {
                name: "SENSOR_CLK".to_string(),
                frequency_mhz: 1,
            },
            sub_resources: vec!["register".to_string(), "interrupt".to_string()],
            capabilities: vec!["Cap::SensorAccess".to_string(), "Cap::MMIO".to_string()],
            coherent_with_cpu: false,
            idle_power_watts: 0.05,
            max_power_watts: 0.5,
            thermal_limit_c: 125.0,
            state_machine: Some(DeviceStateMachine {
                device_name: "Sensor".to_string(),
                states: vec![
                    "Idle".to_string(),
                    "Configuring".to_string(),
                    "Active".to_string(),
                    "Error".to_string(),
                    "Reset".to_string(),
                ],
                initial_state: "Idle".to_string(),
                transitions: vec![
                    StateTransitionRule {
                        from_state: "Idle".to_string(),
                        to_state: "Active".to_string(),
                        trigger: "REG.START = 1".to_string(),
                    },
                    StateTransitionRule {
                        from_state: "Idle".to_string(),
                        to_state: "Configuring".to_string(),
                        trigger: "REG.CFG = 1".to_string(),
                    },
                    StateTransitionRule {
                        from_state: "Configuring".to_string(),
                        to_state: "Active".to_string(),
                        trigger: "REG.START = 1".to_string(),
                    },
                    StateTransitionRule {
                        from_state: "Active".to_string(),
                        to_state: "Idle".to_string(),
                        trigger: "REG.START = 0".to_string(),
                    },
                ],
                operation_requirements: sensor_ops,
                registers: vec![
                    ("START".to_string(), "u32".to_string()),
                    ("CFG".to_string(), "u32".to_string()),
                    ("STATUS".to_string(), "u32".to_string()),
                    ("TEMP_DATA".to_string(), "f32".to_string()),
                ],
            }),
        });

        // Distributed Cluster Endpoints (/N0/F3 and /N1/F3 from Section 3 & 8)
        graph.add_node(HardwareNode {
            endpoint_id: "/N0/F3".to_string(),
            name: "Cluster Node 0 GPU (/N0/F3)".to_string(),
            class: DeviceClass::GPU,
            memory_domain: "VRAM".to_string(),
            preferred_layout: "Tiled2D".to_string(),
            supported_layouts: vec!["Tiled2D".to_string(), "Linear".to_string()],
            clock_domain: ClockDomain {
                name: "N0_GPU_CLK".to_string(),
                frequency_mhz: 1800,
            },
            sub_resources: vec!["memory".to_string(), "dma".to_string(), "queue".to_string()],
            capabilities: vec!["Cap::DMA".to_string(), "Cap::NetworkRDMA".to_string()],
            coherent_with_cpu: false,
            idle_power_watts: 15.0,
            max_power_watts: 250.0,
            thermal_limit_c: 88.0,
            state_machine: None,
        });

        graph.add_node(HardwareNode {
            endpoint_id: "/N1/F3".to_string(),
            name: "Cluster Node 1 GPU (/N1/F3)".to_string(),
            class: DeviceClass::GPU,
            memory_domain: "VRAM".to_string(),
            preferred_layout: "Tiled2D".to_string(),
            supported_layouts: vec!["Tiled2D".to_string(), "Linear".to_string()],
            clock_domain: ClockDomain {
                name: "N1_GPU_CLK".to_string(),
                frequency_mhz: 1800,
            },
            sub_resources: vec!["memory".to_string(), "dma".to_string(), "queue".to_string()],
            capabilities: vec!["Cap::DMA".to_string(), "Cap::NetworkRDMA".to_string()],
            coherent_with_cpu: false,
            idle_power_watts: 15.0,
            max_power_watts: 250.0,
            thermal_limit_c: 88.0,
            state_machine: None,
        });

        // /N2/F3: Distributed Cluster Failover GPU
        graph.add_node(HardwareNode {
            endpoint_id: "/N2/F3".to_string(),
            name: "Cluster Node 2 Failover GPU (/N2/F3)".to_string(),
            class: DeviceClass::GPU,
            memory_domain: "VRAM".to_string(),
            preferred_layout: "Tiled2D".to_string(),
            supported_layouts: vec!["Tiled2D".to_string(), "Linear".to_string()],
            clock_domain: ClockDomain {
                name: "N2_GPU_CLK".to_string(),
                frequency_mhz: 1800,
            },
            sub_resources: vec!["memory".to_string(), "dma".to_string(), "queue".to_string()],
            capabilities: vec!["Cap::DMA".to_string(), "Cap::NetworkRDMA".to_string()],
            coherent_with_cpu: false,
            idle_power_watts: 15.0,
            max_power_watts: 250.0,
            thermal_limit_c: 88.0,
            state_machine: None,
        });

        // /F8: Dedicated NPU Tensor Processing Unit (Phase 3)
        graph.add_node(HardwareNode {
            endpoint_id: "/F8".to_string(),
            name: "Tensor Processing Unit (TPU/NPU Matrix Core)".to_string(),
            class: DeviceClass::NPU,
            memory_domain: "DeviceRAM".to_string(),
            preferred_layout: "Tensor4D".to_string(),
            supported_layouts: vec![
                "Tensor4D".to_string(),
                "Linear".to_string(),
                "Tiled2D".to_string(),
                "Packed".to_string(),
            ],
            clock_domain: ClockDomain {
                name: "NPU_CLK".to_string(),
                frequency_mhz: 1200,
            },
            sub_resources: vec![
                "tensor_engine".to_string(),
                "dma".to_string(),
                "queue".to_string(),
                "matrix_accum".to_string(),
            ],
            capabilities: vec![
                "Cap::Compute".to_string(),
                "Cap::DMA".to_string(),
                "Cap::NPUTensor".to_string(),
            ],
            coherent_with_cpu: false,
            idle_power_watts: 8.0,
            max_power_watts: 120.0,
            thermal_limit_c: 85.0,
            state_machine: None,
        });

        // Physical & Logical Edges
        // CPU <-> SystemRAM (Coherent Memory Bus)
        graph.add_bidirectional_edge(
            "/F1",
            "/F2",
            "DDR5_CoherentBus",
            89.6,
            65,
            true,
            Some("/F2::dma"),
            true,
            true,
            120.0,
            ClockRelation::Synchronous,
            None,
        );

        // SystemRAM <-> GPU (PCIe Gen4 x16 DMA)
        graph.add_bidirectional_edge(
            "/F2",
            "/F3",
            "PCIe_Gen4x16_DMA",
            31.5,
            850,
            true,
            Some("/F3::dma"),
            false,
            false,
            450.0,
            ClockRelation::Mesochronous,
            Some("Cap::DMA"),
        );

        // CPU <-> GPU (MMIO / Command Queue over PCIe)
        graph.add_bidirectional_edge(
            "/F1",
            "/F3",
            "PCIe_MMIO_Queue",
            16.0,
            900,
            true,
            Some("/F3::dma"),
            false,
            true,
            400.0,
            ClockRelation::Mesochronous,
            None,
        );

        // Storage (/F4) <-> GPU (/F3) Direct Peer-to-Peer (NVMe -> GPU P2P DMA)
        graph.add_bidirectional_edge(
            "/F4",
            "/F3",
            "PCIe_P2P_Direct",
            28.0,
            620,
            true,
            Some("/F3::dma"),
            false,
            false,
            350.0,
            ClockRelation::Mesochronous,
            Some("Cap::PeerMemory"),
        );

        // Storage (/F4) <-> SystemRAM (/F2) Standard NVMe DMA
        graph.add_bidirectional_edge(
            "/F4",
            "/F2",
            "NVMe_PCIe_DMA",
            7.2,
            1200,
            true,
            Some("/F4::dma"),
            false,
            false,
            280.0,
            ClockRelation::Mesochronous,
            Some("Cap::DMA"),
        );

        // NIC (/F5) <-> SystemRAM (/F2)
        graph.add_bidirectional_edge(
            "/F5",
            "/F2",
            "PCIe_NIC_DMA",
            12.5,
            1100,
            true,
            Some("/F5::dma"),
            false,
            false,
            300.0,
            ClockRelation::Asynchronous,
            Some("Cap::DMA"),
        );

        // FPGA (/F6) <-> CPU (/F1) Asynchronous AXI-Lite / PCIe Bridge
        graph.add_bidirectional_edge(
            "/F6",
            "/F1",
            "AXI4_AsyncBridge",
            8.0,
            450,
            true,
            Some("/F6::dma"),
            false,
            true,
            200.0,
            ClockRelation::Asynchronous,
            None,
        );

        // FPGA (/F6) <-> SystemRAM (/F2)
        graph.add_bidirectional_edge(
            "/F6",
            "/F2",
            "FPGA_SG_DMA",
            14.0,
            550,
            true,
            Some("/F6::dma"),
            false,
            false,
            220.0,
            ClockRelation::Asynchronous,
            Some("Cap::DMA"),
        );

        // Sensor (/F7) <-> CPU (/F1) I2C Bus
        graph.add_bidirectional_edge(
            "/F7",
            "/F1",
            "I2C_Bus_400kHz",
            0.0004,
            15000,
            false,
            None,
            false,
            true,
            15.0,
            ClockRelation::Asynchronous,
            Some("Cap::SensorAccess"),
        );

        // Distributed Cluster link: /N0/F3 <-> /N1/F3 via RDMA RoCEv2
        graph.add_bidirectional_edge(
            "/N0/F3",
            "/N1/F3",
            "RDMA_RoCEv2_100G",
            12.5,
            2400,
            true,
            Some("/N0/F3::dma"),
            false,
            false,
            600.0,
            ClockRelation::Asynchronous,
            Some("Cap::NetworkRDMA"),
        );

        // Distributed Cluster Failover link: /N1/F3 <-> /N2/F3
        graph.add_bidirectional_edge(
            "/N1/F3",
            "/N2/F3",
            "RDMA_RoCEv2_100G",
            12.5,
            2400,
            true,
            Some("/N1/F3::dma"),
            false,
            false,
            600.0,
            ClockRelation::Asynchronous,
            Some("Cap::NetworkRDMA"),
        );

        // NPU (/F8) <-> SystemRAM (/F2) PCIe Gen4 x8 DMA
        graph.add_bidirectional_edge(
            "/F8",
            "/F2",
            "PCIe_Gen4x8_NPU_DMA",
            15.8,
            720,
            true,
            Some("/F8::dma"),
            false,
            false,
            320.0,
            ClockRelation::Mesochronous,
            Some("Cap::DMA"),
        );

        // NPU (/F8) <-> GPU (/F3) NVLink/P2P Direct Bridge
        graph.add_bidirectional_edge(
            "/F8",
            "/F3",
            "NVLink_P2P_Direct",
            32.0,
            380,
            true,
            Some("/F8::dma"),
            false,
            false,
            290.0,
            ClockRelation::Mesochronous,
            Some("Cap::PeerMemory"),
        );

        graph
    }

    pub fn add_node(&mut self, node: HardwareNode) {
        let id = node.endpoint_id.clone();
        if !self.node_order.contains(&id) {
            self.node_order.push(id.clone());
        }
        self.nodes.insert(id, node);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_bidirectional_edge(
        &mut self,
        a: &str,
        b: &str,
        protocol: &str,
        bandwidth_gbps: f64,
        latency_ns: u64,
        dma_available: bool,
        dma_engine: Option<&str>,
        coherent: bool,
        addressable: bool,
        power_mw_per_gb: f64,
        clock_relation: ClockRelation,
        required_capability: Option<&str>,
    ) {
        self.edges.push(HardwareEdge {
            from: a.to_string(),
            to: b.to_string(),
            protocol: protocol.to_string(),
            bandwidth_gbps,
            latency_ns,
            dma_available,
            dma_engine: dma_engine.map(|s| s.to_string()),
            coherent,
            addressable,
            power_mw_per_gb,
            clock_relation: clock_relation.clone(),
            required_capability: required_capability.map(|s| s.to_string()),
            online: true,
        });
        self.edges.push(HardwareEdge {
            from: b.to_string(),
            to: a.to_string(),
            protocol: protocol.to_string(),
            bandwidth_gbps,
            latency_ns,
            dma_available,
            dma_engine: dma_engine.map(|s| s.to_string()),
            coherent,
            addressable,
            power_mw_per_gb,
            clock_relation,
            required_capability: required_capability.map(|s| s.to_string()),
            online: true,
        });
    }

    /// Resolve a capability query like `find(CPU)`, `find(GPU)`, `find(Storage)` to an endpoint ID.
    pub fn find_by_class(&self, class_name: &str) -> Option<&HardwareNode> {
        let target = DeviceClass::from_name(class_name);
        for id in &self.node_order {
            if let Some(node) = self.nodes.get(id) {
                if node.class == target {
                    return Some(node);
                }
            }
        }
        None
    }

    pub fn get_node(&self, endpoint_str: &str) -> Option<&HardwareNode> {
        let ep = Endpoint::parse(endpoint_str);
        let base = ep.base_id();
        self.nodes.get(&base)
    }

    /// Determine clock relationship between two endpoints (Section 6).
    pub fn clock_relation(&self, src_ep: &str, dst_ep: &str) -> ClockRelation {
        let s_base = Endpoint::parse(src_ep).base_id();
        let d_base = Endpoint::parse(dst_ep).base_id();
        if s_base == d_base {
            return ClockRelation::Synchronous;
        }
        if let (Some(sn), Some(dn)) = (self.nodes.get(&s_base), self.nodes.get(&d_base)) {
            if sn.clock_domain.name == dn.clock_domain.name {
                return ClockRelation::Synchronous;
            }
        }
        for e in &self.edges {
            if e.from == s_base && e.to == d_base {
                return e.clock_relation.clone();
            }
        }
        ClockRelation::Asynchronous
    }

    /// Plan an optimal route (and fallback route) between two endpoints (Sections 3, 8, 11).
    pub fn plan_route(&self, src_ep: &str, dst_ep: &str) -> Result<PlannedRoute, String> {
        let src = Endpoint::parse(src_ep).base_id();
        let dst = Endpoint::parse(dst_ep).base_id();

        if !self.nodes.contains_key(&src) {
            return Err(format!(
                "Source endpoint `{}` does not exist in the Hardware Graph",
                src
            ));
        }
        if !self.nodes.contains_key(&dst) {
            return Err(format!(
                "Destination endpoint `{}` does not exist in the Hardware Graph",
                dst
            ));
        }

        // Enumerate simple paths up to 3 hops
        let mut valid_paths: Vec<(Vec<String>, Vec<&HardwareEdge>)> = Vec::new();
        let mut rejected: Vec<(Vec<String>, String)> = Vec::new();

        // 1-hop direct edges
        for e1 in &self.edges {
            if e1.from == src && e1.to == dst {
                if !e1.online {
                    rejected.push((
                        vec![src.clone(), dst.clone()],
                        format!(
                            "Direct link ({}) is currently offline/disconnected",
                            e1.protocol
                        ),
                    ));
                } else if let Some(req_cap) = &e1.required_capability {
                    if !self.granted_capabilities.contains(req_cap) {
                        rejected.push((
                            vec![src.clone(), dst.clone()],
                            format!(
                                "Blocked by missing capability `{}` on {}",
                                req_cap, e1.protocol
                            ),
                        ));
                    } else {
                        valid_paths.push((vec![src.clone(), dst.clone()], vec![e1]));
                    }
                } else {
                    valid_paths.push((vec![src.clone(), dst.clone()], vec![e1]));
                }
            }
        }

        // 2-hop paths (e.g., /F4 -> /F2 -> /F3 through SystemRAM)
        for e1 in &self.edges {
            if e1.from == src && e1.to != dst && e1.online {
                let mid = &e1.to;
                for e2 in &self.edges {
                    if e2.from == *mid && e2.to == dst && e2.online {
                        let cap1_ok = e1
                            .required_capability
                            .as_ref()
                            .map(|c| self.granted_capabilities.contains(c))
                            .unwrap_or(true);
                        let cap2_ok = e2
                            .required_capability
                            .as_ref()
                            .map(|c| self.granted_capabilities.contains(c))
                            .unwrap_or(true);
                        if cap1_ok && cap2_ok {
                            valid_paths
                                .push((vec![src.clone(), mid.clone(), dst.clone()], vec![e1, e2]));
                        }
                    }
                }
            }
        }

        if valid_paths.is_empty() {
            let reason = if rejected.is_empty() {
                format!("No physical or logical link connects {} to {}", src, dst)
            } else {
                rejected
                    .iter()
                    .map(|(p, r)| format!("{} ({})", p.join(" -> "), r))
                    .collect::<Vec<_>>()
                    .join("; ")
            };
            return Err(reason);
        }

        // Sort valid paths by highest effective bandwidth, then lowest latency
        valid_paths.sort_by(|a, b| {
            let bw_a =
                a.1.iter()
                    .map(|e| e.bandwidth_gbps)
                    .fold(f64::INFINITY, f64::min);
            let bw_b =
                b.1.iter()
                    .map(|e| e.bandwidth_gbps)
                    .fold(f64::INFINITY, f64::min);
            bw_b.partial_cmp(&bw_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        let (best_hops, best_edges) = &valid_paths[0];
        let fallback_hops = if valid_paths.len() > 1 {
            Some(valid_paths[1].0.clone())
        } else {
            None
        };

        let mut min_bw = f64::INFINITY;
        let mut bottleneck = String::new();
        let mut total_latency = 0u64;
        let mut total_power = 0.0f64;
        let mut protocols = Vec::new();
        let mut dma_engines = Vec::new();
        let mut needs_barrier = false;
        let mut async_clock = false;

        for edge in best_edges {
            if edge.bandwidth_gbps < min_bw {
                min_bw = edge.bandwidth_gbps;
                bottleneck = format!(
                    "{} ({} -> {} @ {:.1} GB/s)",
                    edge.protocol, edge.from, edge.to, edge.bandwidth_gbps
                );
            }
            total_latency += edge.latency_ns;
            total_power += (edge.power_mw_per_gb * (edge.bandwidth_gbps.min(10.0))) / 1000.0;
            protocols.push(edge.protocol.clone());
            if let Some(dma) = &edge.dma_engine {
                if !dma_engines.contains(dma) {
                    dma_engines.push(dma.clone());
                }
            }
            if !edge.coherent {
                needs_barrier = true;
            }
            if edge.clock_relation == ClockRelation::Asynchronous {
                async_clock = true;
            }
        }

        Ok(PlannedRoute {
            source: src,
            destination: dst,
            hops: best_hops.clone(),
            protocols,
            dma_engines,
            effective_bandwidth_gbps: min_bw,
            total_latency_ns: total_latency,
            estimated_power_watts: (total_power * 100.0).round() / 100.0,
            requires_memory_barrier: needs_barrier,
            crosses_async_clock: async_clock,
            fallback_hops,
            rejected_alternatives: rejected,
            bottleneck_link: bottleneck,
        })
    }
}
