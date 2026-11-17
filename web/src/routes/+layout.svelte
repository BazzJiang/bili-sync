<script lang="ts">
	import '../app.css';
	import AppSidebar from '$lib/components/app-sidebar.svelte';
	import BreadCrumb from '$lib/components/bread-crumb.svelte';
	import CommandPalette from '$lib/components/command-palette.svelte';
	import SearchIcon from '@lucide/svelte/icons/search';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Separator } from '$lib/components/ui/separator/index.js';
	import { breadcrumbStore } from '$lib/stores/breadcrumb';
	import * as Sidebar from '$lib/components/ui/sidebar/index.js';
	import { Toaster } from '$lib/components/ui/sonner/index.js';
	import { ModeWatcher } from 'mode-watcher';
	import { authStatus } from '$lib/stores/auth';
	import { goto } from '$app/navigation';

	// 命令面板的开关状态：header 触发按钮与组件内的 ⌘K 共用
	let paletteOpen = false;
</script>

<ModeWatcher disableHeadScriptInjection />
<Toaster position="top-center" duration={3000} />
<CommandPalette bind:open={paletteOpen} />
<Sidebar.Provider>
	<AppSidebar />
	<Sidebar.Inset class="flex flex-col" style="height: calc(100vh - 1rem)">
		<header class="flex h-16 shrink-0 items-center gap-2">
			<div class="flex items-center gap-2 px-4">
				<Sidebar.Trigger class="-ml-1" />
				<Separator orientation="vertical" class="mr-2 data-[orientation=vertical]:h-4" />
				<BreadCrumb items={$breadcrumbStore} />
			</div>
			<Button
				variant="outline"
				size="sm"
				class="text-muted-foreground mr-4 ml-auto h-8 gap-2 font-normal"
				aria-label="打开命令面板"
				onclick={() => (paletteOpen = true)}
			>
				<SearchIcon class="size-3.5" />
				<span class="hidden text-xs sm:inline">搜索</span>
				<kbd
					class="bg-muted text-muted-foreground pointer-events-none rounded px-1.5 py-0.5 font-mono text-[10px]"
					>⌘K</kbd
				>
			</Button>
		</header>
		<div
			class="w-full overflow-y-auto px-6 py-2"
			style="scrollbar-width: thin; scrollbar-gutter: stable !important;"
			id="main"
		>
			<slot />
		</div>
		{#if $authStatus === 'unauthorized'}
			<div
				class="border-b border-amber-300/60 bg-amber-50 px-4 py-2 text-sm text-amber-900 dark:border-amber-500/30 dark:bg-amber-950/40 dark:text-amber-200"
			>
				<div class="flex flex-wrap items-center gap-2">
					<span class="font-medium">当前未通过认证</span>
					<span class="text-amber-800/80 dark:text-amber-200/80">
						页面数据无法加载，请到设置中填写访问 Token。
					</span>
					<Button
						variant="outline"
						size="sm"
						class="ml-auto h-7 border-amber-300 bg-amber-100 text-xs text-amber-900 hover:bg-amber-200 dark:border-amber-500/40 dark:bg-amber-900/40 dark:text-amber-100"
						onclick={() => goto('/settings')}
					>
						去设置认证
					</Button>
				</div>
			</div>
		{/if}
	</Sidebar.Inset>
</Sidebar.Provider>
