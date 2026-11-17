import { writable } from 'svelte/store';

// 前端认证状态：用于在未认证时给出明确提示，而不是让页面停在"加载中"
export type AuthStatus = 'unknown' | 'ok' | 'unauthorized';

export const authStatus = writable<AuthStatus>('unknown');
