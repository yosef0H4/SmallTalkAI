import { t } from './i18n';
import type { Runtime } from './modules.svelte';

const runtimeNames: Readonly<Record<string, string>> = {
  vulkan: 'Vulkan',
  cuda: 'CUDA',
  webgpu: 'WebGPU',
  cpu: 'CPU',
};

export function runtimeLabel(runtime: Pick<Runtime, 'backend' | 'label'>): string {
  return runtimeNames[runtime.backend] ?? runtime.label;
}

export function runtimeHardware(runtime: Pick<Runtime, 'backend' | 'asset'>): string {
  if (runtime.asset === 'parakeet-cuda-windows') return t('module.runtime.blackwell');
  return Object.hasOwn(runtimeNames, runtime.backend) ? t(`module.runtime.${runtime.backend}`) : '';
}

export function recommendationLabel(reason?: string | null): string {
  return reason ? t(`module.recommendation.${reason}`) : t('ui.recommended.9ef937');
}
