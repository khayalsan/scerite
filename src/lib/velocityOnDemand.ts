import { invoke } from "@tauri-apps/api/core";

export interface VelocityOnDemandStatus {
  enabled: boolean;
  publicPort: number | null;
  backendPort: number | null;
  proxyRunning: boolean;
}

export const velocityOnDemand = {
  status: (profileId: string) => invoke<VelocityOnDemandStatus>("velocity_on_demand_status", { profileId }),
  enable: (profileId: string) => invoke<VelocityOnDemandStatus>("enable_velocity_on_demand", { profileId }),
  disable: (profileId: string) => invoke<VelocityOnDemandStatus>("disable_velocity_on_demand", { profileId }),
};
