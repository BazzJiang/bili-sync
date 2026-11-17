<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { goto } from '$app/navigation';
	import ActivityIcon from '@lucide/svelte/icons/activity';
	import ChartPieIcon from '@lucide/svelte/icons/chart-pie';
	import CloudDownloadIcon from '@lucide/svelte/icons/cloud-download';
	import DatabaseIcon from '@lucide/svelte/icons/database';
	import FilePlayIcon from '@lucide/svelte/icons/file-play';
	import CommandIcon from '@lucide/svelte/icons/command';
	import PaletteIcon from '@lucide/svelte/icons/palette';
	import Settings2Icon from '@lucide/svelte/icons/settings-2';
	import SquareTerminalIcon from '@lucide/svelte/icons/square-terminal';
	import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
	import VideoIcon from '@lucide/svelte/icons/video';
	import * as Command from '$lib/components/ui/command/index.js';
	import api from '$lib/api';
	import type { ApiError, VideoInfo } from '$lib/types';
	import { toggleMode } from 'mode-watcher';
	import { toast } from 'svelte-sonner';

	// 面板开关由 layout 绑定：layout header 的触发按钮与 ⌘K 都通过它打开面板
	export let open: boolean = false;

	// 与侧边栏保持一致的一级页面
	const navItems = [
		{ label: '仪表盘', href: '/', icon: ChartPieIcon },
		{ label: '活动', href: '/activity', icon: ActivityIcon },
		{ label: '日志', href: '/logs', icon: SquareTerminalIcon },
		{ label: '视频', href: '/videos', icon: FilePlayIcon },
		{ label: '失败视频', href: '/wanted', icon: TriangleAlertIcon },
		{ label: '视频源', href: '/video-sources', icon: DatabaseIcon },
		{ label: '设置', href: '/settings', icon: Settings2Icon }
	];

	let query = '';
	let searchTerm = '';
	let searchResults: VideoInfo[] = [];
	let searching = false;
	let searchFailed = false;
	let searchTimer: ReturnType<typeof setTimeout> | undefined;
	// 只接受最后一次请求的响应，避免快速输入时结果错乱
	let searchRequestId = 0;
	let wasOpen = false;

	// 输入至少 2 个字符才发起搜索，防抖 300ms
	function handleQueryChange(term: string) {
		clearTimeout(searchTimer);
		searchFailed = false;
		if (term.length < 2) {
			searching = false;
			searchResults = [];
			searchRequestId += 1;
			return;
		}
		searching = true;
		searchTimer = setTimeout(() => {
			void runSearch(term);
		}, 300);
	}

	async function runSearch(term: string) {
		const requestId = ++searchRequestId;
		try {
			const response = await api.getVideos({ query: term, page: 0, page_size: 8 });
			if (requestId !== searchRequestId) return;
			searchResults = response.data.videos ?? [];
		} catch (error) {
			if (requestId !== searchRequestId) return;
			console.error('搜索视频失败：', error);
			searchResults = [];
			searchFailed = true;
		} finally {
			if (requestId === searchRequestId) searching = false;
		}
	}

	function resetSearch() {
		clearTimeout(searchTimer);
		searchRequestId += 1;
		query = '';
		searchTerm = '';
		searchResults = [];
		searching = false;
		searchFailed = false;
	}

	function focusSearchInput() {
		void tick().then(() => {
			const input = document.querySelector<HTMLInputElement>('[data-slot="command-input"]');
			if (input) {
				input.focus();
				return;
			}
			// Dialog 内容走 portal，极少数情况下这一帧还没挂载完成，稍后再试一次
			setTimeout(() => {
				document.querySelector<HTMLInputElement>('[data-slot="command-input"]')?.focus();
			}, 50);
		});
	}

	function navigateTo(href: string) {
		open = false;
		void goto(href);
	}

	async function triggerDownload() {
		open = false;
		try {
			await api.triggerDownloadTask();
			toast.success('已触发下载任务', {
				description: '任务将立即开始执行'
			});
		} catch (error) {
			console.error('触发下载任务失败：', error);
			toast.error('触发下载任务失败', {
				description: (error as ApiError).message
			});
		}
	}

	function toggleTheme() {
		open = false;
		toggleMode();
	}

	$: searchTerm = query.trim();
	$: handleQueryChange(searchTerm);

	// 面板关闭后清空搜索状态，下次打开是干净的
	$: if (open !== wasOpen) {
		wasOpen = open;
		resetSearch();
		if (open) focusSearchInput();
	}

	onMount(() => {
		// window 只在客户端可用的生命周期内访问
		const handleKeydown = (event: KeyboardEvent) => {
			if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
				event.preventDefault();
				open = !open;
				return;
			}
			if (event.key === 'Escape' && open) {
				event.preventDefault();
				open = false;
			}
		};
		window.addEventListener('keydown', handleKeydown);
		return () => window.removeEventListener('keydown', handleKeydown);
	});
</script>

<Command.Dialog bind:open title="命令面板" description="搜索页面、执行快捷动作或查找视频">
	<Command.Input bind:value={query} autofocus placeholder="搜索页面、快捷动作或视频…" />
	<Command.List class="max-h-[360px]">
		{#if searching && searchResults.length === 0}
			<Command.Loading class="text-muted-foreground py-6 text-center text-sm">
				正在搜索视频…
			</Command.Loading>
		{:else}
			<Command.Empty class="text-muted-foreground">
				{#if searchFailed}
					搜索视频失败，请稍后重试
				{:else if searchTerm.length < 2}
					输入至少 2 个字符即可搜索视频
				{:else}
					未找到匹配的命令或视频
				{/if}
			</Command.Empty>
		{/if}

		<Command.Group heading="导航">
			{#each navItems as item (item.href)}
				<Command.Item value={`${item.label} ${item.href}`} onSelect={() => navigateTo(item.href)}>
					<item.icon class="size-4" />
					<span>{item.label}</span>
				</Command.Item>
			{/each}
		</Command.Group>

		<Command.Separator />

		<Command.Group heading="快捷动作">
			<Command.Item value="立即执行下载任务 下载 同步" onSelect={triggerDownload}>
				<CloudDownloadIcon class="size-4" />
				<span>立即执行下载任务</span>
			</Command.Item>
			<Command.Item value="切换亮色 暗色 主题 theme" onSelect={toggleTheme}>
				<PaletteIcon class="size-4" />
				<span>切换亮色 / 暗色</span>
			</Command.Item>
			<Command.Item value="切换命令面板 快捷键 command palette" onSelect={() => (open = !open)}>
				<CommandIcon class="size-4" />
				<span>切换命令面板</span>
				<Command.Shortcut>⌘K</Command.Shortcut>
			</Command.Item>
		</Command.Group>

		{#if searchResults.length > 0}
			<Command.Separator />
			<Command.Group heading="视频搜索">
				{#each searchResults as video (video.id)}
					<Command.Item
						value={`${video.name} ${video.bvid}`}
						onSelect={() => navigateTo(`/video/${video.id}`)}
					>
						<VideoIcon class="size-4" />
						<div class="flex min-w-0 flex-1 flex-col">
							<span class="truncate">{video.name}</span>
							<span class="text-muted-foreground truncate text-xs">{video.upper_name}</span>
						</div>
						<Command.Shortcut>↵</Command.Shortcut>
					</Command.Item>
				{/each}
			</Command.Group>
		{/if}
	</Command.List>
</Command.Dialog>
