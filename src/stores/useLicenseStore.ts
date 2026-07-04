import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';

export interface LicenseFile {
    license_key: string;
    status: string;
    plan: string;
    expires_at: string;
    max_devices: number;
    features: string[];
    hwid: string;
    device_name?: string;
    issued_at: string;
    last_heartbeat: string;
}

export type LicenseState = 
    | { type: 'Active'; data: LicenseFile }
    | { type: 'Expired' }
    | { type: 'Revoked' }
    | { type: 'NoLicense' }
    | { type: 'Invalid'; error: string };

export const useLicenseStore = defineStore('license', () => {
    const rawState = ref<any>({ NoLicense: null }); // matches Rust enum format
    const isReady = ref(false);
    const isLoading = ref(false);
    const errorMsg = ref<string | null>(null);

    const licenseStatus = computed<LicenseState>(() => {
        if (!rawState.value) return { type: 'NoLicense' };
        
        if (typeof rawState.value === 'string') {
            // Expired, Revoked, NoLicense are usually serialized as string enum if no payload
            return { type: rawState.value as 'Expired' | 'Revoked' | 'NoLicense' };
        }
        
        // Active or Invalid have payloads
        if (rawState.value.Active) {
            return { type: 'Active', data: rawState.value.Active as LicenseFile };
        }
        
        if (rawState.value.Invalid) {
            return { type: 'Invalid', error: rawState.value.Invalid as string };
        }
        
        return { type: 'NoLicense' };
    });

    const isReadOnly = computed(() => {
        return licenseStatus.value.type !== 'Active';
    });

    const isDev = computed(() => {
        const status = licenseStatus.value;
        return status.type === 'Active' && status.data.plan === 'dev';
    });

    const isPro = computed(() => {
        const status = licenseStatus.value;
        return status.type === 'Active' && status.data.plan === 'pro';
    });
    
    const isTrial = computed(() => {
        const status = licenseStatus.value;
        return status.type === 'Active' && status.data.plan === 'trial';
    });

    const daysLeft = computed(() => {
        const status = licenseStatus.value;
        if (status.type === 'Active') {
            const expiresAt = new Date(status.data.expires_at).getTime();
            const now = new Date().getTime();
            const diff = expiresAt - now;
            return Math.max(0, Math.ceil(diff / (1000 * 60 * 60 * 24)));
        }
        return 0;
    });

    async function checkState() {
        try {
            isLoading.value = true;
            errorMsg.value = null;
            const res = await invoke('get_license_state');
            rawState.value = res;
        } catch (e: any) {
            console.error("Failed to check license state", e);
            errorMsg.value = e.toString();
        } finally {
            isLoading.value = false;
            isReady.value = true;
        }
    }

    async function activateTrial() {
        try {
            isLoading.value = true;
            errorMsg.value = null;
            const res = await invoke('activate_trial');
            rawState.value = res;
            return true;
        } catch (e: any) {
            errorMsg.value = e.toString();
            return false;
        } finally {
            isLoading.value = false;
        }
    }

    async function activateKey(key: string) {
        try {
            isLoading.value = true;
            errorMsg.value = null;
            const res = await invoke('activate_license_key', { key });
            rawState.value = res;
            return true;
        } catch (e: any) {
            errorMsg.value = e.toString();
            return false;
        } finally {
            isLoading.value = false;
        }
    }

    async function deactivate() {
        try {
            isLoading.value = true;
            await invoke('deactivate_license');
            await checkState();
            return true;
        } catch (e: any) {
            errorMsg.value = e.toString();
            return false;
        } finally {
            isLoading.value = false;
        }
    }

    async function refresh() {
        try {
            isLoading.value = true;
            const res = await invoke('refresh_license');
            rawState.value = res;
        } catch (e: any) {
            console.error("Failed to refresh license", e);
        } finally {
            isLoading.value = false;
        }
    }

    return {
        licenseStatus,
        isReady,
        isLoading,
        errorMsg,
        isReadOnly,
        isPro,
        isTrial,
        isDev,
        daysLeft,
        checkState,
        activateTrial,
        activateKey,
        deactivate,
        refresh
    };
});
