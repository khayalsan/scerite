import { invoke } from "@tauri-apps/api/core";

export interface VelocityOnDemandStatus {
  enabled: boolean;
  publicPort: number | null;
  backendPort: number | null;
  proxyRunning: boolean;
}

export interface VelocityProxyUsage {
  running: boolean;
  cpuPercent: number | null;
  memoryBytes: number | null;
}

export const velocityOnDemand = {
  status: (profileId: string) => invoke<VelocityOnDemandStatus>("velocity_on_demand_status", { profileId }),
  usage: (profileId: string) => invoke<VelocityProxyUsage>("velocity_proxy_usage", { profileId }),
  enable: (profileId: string) => invoke<VelocityOnDemandStatus>("enable_velocity_on_demand", { profileId }),
  disable: (profileId: string) => invoke<VelocityOnDemandStatus>("disable_velocity_on_demand", { profileId }),
};
