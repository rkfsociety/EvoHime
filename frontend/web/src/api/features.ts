import { apiRequest } from "./client";

export type FeatureFlags = {
  sites: boolean;
  scheduled: boolean;
  otlp: boolean;
  cloud_sync: boolean;
};

export function getFeatures() {
  return apiRequest<FeatureFlags>("/api/features");
}
