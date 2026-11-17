<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Switch } from '$lib/components/ui/switch/index.js';
	import api from '$lib/api';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';
	import type { ApiError, TaskStatus, VideoInfo } from '$lib/types';
	import { toast } from 'svelte-sonner';
	import ArrowRightIcon from '@lucide/svelte/icons/arrow-right';
	import CalendarIcon from '@lucide/svelte/icons/calendar';
	import CircleCheckBigIcon from '@lucide/svelte/icons/circle-check-big';
	import CloudDownloadIcon from '@lucide/svelte/icons/cloud-download';
	import ListVideoIcon from '@lucide/svelte/icons/list-video';
	import PlayIcon from '@lucide/svelte/icons/play';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import UserIcon from '@lucide/svelte/icons/user';

	// 自动刷新间隔：15 秒
	const AUTO_REFRESH_INTERVAL = 15000;
	// 两个列表一次拉取的条数
	const LIST_PAGE_SIZE = 10;
	// 自动刷新开关的本地记忆键
	const AUTO_REFRESH_STORAGE_KEY = 'bili-sync.activity.auto-refresh';
	// 子任务名称，与 video-list-row 保持一致
	const TASK_NAMES = ['视频封面', '视频信息', 'UP主头像', 'UP主信息', '分页下载'];

	let taskStatus: TaskStatus | null = null;
	let triggering = false;

	let waitingVideos: VideoInfo[] = [];
	let waitingTotal = 0;
	let recentVideos: VideoInfo[] = [];

	let initialLoading = true;
	let refreshing = false;
	let loadFailed = false;
	let lastRefresh: Date | null = null;

	let autoRefresh = true;
	let refreshTimer: ReturnType<typeof setInterval> | null = null;
	let unsubscribeTasks: (() => void) | null = null;
	// 错误 toast 节流：自动刷新失败时避免每 15 秒弹一次
	let lastErrorAt = 0;

	function formatDateTime(value: Date | string | null | undefined): string {
		if (!value) return '-';
		const date = value instanceof Date ? value : new Date(value);
		if (Number.isNaN(date.getTime())) return '-';
		return date.toLocaleString('zh-CN');
	}

	function formatTimeOfDay(value: Date | null): string {
		if (!value) return '--:--:--';
		return value.toLocaleTimeString('zh-CN', {
			hour: '2-digit',
			minute: '2-digit',
			second: '2-digit',
			hour12: false
		});
	}

	function getStatusText(status: number): string {
		if (status === 7) {
			return '已完成';
		} else if (status === 0) {
			return '未开始';
		} else {
			return `失败${status}次`;
		}
	}

	// 分段进度条颜色规则，与 video-list-row 保持一致：7=绿 / 0=黄 / 其他=红
	function getSegmentColor(status: number): string {
		if (status === 7) {
			return 'bg-emerald-500';
		} else if (status === 0) {
			return 'bg-yellow-500';
		} else {
			return 'bg-rose-500';
		}
	}

	function getTaskName(index: number): string {
		return TASK_NAMES[index] || `任务${index + 1}`;
	}

	function getProgressTitle(video: VideoInfo): string {
		return video.download_status
			.map((status, index) => `${getTaskName(index)}：${getStatusText(status)}`)
			.join('\n');
	}

	function getCompletedCount(downloadStatus: number[]): number {
		return downloadStatus.filter((status) => status === 7).length;
	}

	// 整体状态语义与 video-list-row 保持一致
	function getOverallStatus(video: VideoInfo): { text: string; style: string } {
		if (!video.valid) {
			return { text: '失效', style: 'bg-gray-100 text-gray-700' };
		}
		if (!video.should_download) {
			return { text: '跳过', style: 'bg-gray-100 text-gray-700' };
		}
		const completed = getCompletedCount(video.download_status);
		const total = video.download_status.length;
		const failed = video.download_status.filter((status) => status !== 7 && status !== 0).length;

		if (completed === total) {
			return { text: '完成', style: 'bg-emerald-700 text-emerald-100' };
		} else if (failed > 0) {
			return { text: '失败', style: 'bg-rose-700 text-rose-100' };
		} else {
			return { text: '等待', style: 'bg-yellow-700 text-yellow-100' };
		}
	}

	async function loadLists(showLoading = false, alwaysNotify = false) {
		if (refreshing) return;
		refreshing = true;
		if (showLoading) initialLoading = true;
		try {
			const [waiting, recent] = await Promise.all([
				api.getVideos({ status_filter: 'waiting', page: 0, page_size: LIST_PAGE_SIZE }),
				api.getVideos({ page: 0, page_size: LIST_PAGE_SIZE })
			]);
			waitingVideos = waiting.data.videos;
			waitingTotal = waiting.data.total_count;
			recentVideos = recent.data.videos;
			loadFailed = false;
			lastRefresh = new Date();
		} catch (error) {
			console.error('加载活动数据失败：', error);
			loadFailed = true;
			const now = Date.now();
			if (alwaysNotify || now - lastErrorAt > 30000) {
				lastErrorAt = now;
				toast.error('加载活动数据失败', {
					description: (error as ApiError).message
				});
			}
		} finally {
			refreshing = false;
			initialLoading = false;
		}
	}

	async function handleTriggerDownload() {
		triggering = true;
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
		} finally {
			triggering = false;
		}
	}

	function stopAutoRefresh() {
		if (refreshTimer) {
			clearInterval(refreshTimer);
			refreshTimer = null;
		}
	}

	function startAutoRefresh() {
		stopAutoRefresh();
		if (!autoRefresh) return;
		refreshTimer = setInterval(() => {
			// 页面不可见时跳过，避免后台标签页无意义请求
			if (document.hidden) return;
			loadLists(false);
		}, AUTO_REFRESH_INTERVAL);
	}

	function handleAutoRefreshChange(next: boolean) {
		autoRefresh = next;
		try {
			localStorage.setItem(AUTO_REFRESH_STORAGE_KEY, next ? '1' : '0');
		} catch (error) {
			console.error('保存自动刷新设置失败：', error);
		}
		if (next) {
			startAutoRefresh();
			loadLists(false);
		} else {
			stopAutoRefresh();
		}
	}

	function handleVisibilityChange() {
		if (!autoRefresh || document.hidden) return;
		// 从后台切回时立即刷新一次，并重置计时
		loadLists(false);
		startAutoRefresh();
	}

	onMount(() => {
		setBreadcrumb([{ label: '活动' }]);

		try {
			const stored = localStorage.getItem(AUTO_REFRESH_STORAGE_KEY);
			if (stored !== null) {
				autoRefresh = stored === '1';
			}
		} catch (error) {
			console.error('读取自动刷新设置失败：', error);
		}

		unsubscribeTasks = api.subscribeToTasks((data: TaskStatus) => {
			taskStatus = data;
		});

		loadLists(true);
		startAutoRefresh();
		document.addEventListener('visibilitychange', handleVisibilityChange);
	});

	onDestroy(() => {
		stopAutoRefresh();
		document.removeEventListener('visibilitychange', handleVisibilityChange);
		if (unsubscribeTasks) {
			unsubscribeTasks();
			unsubscribeTasks = null;
		}
	});
</script>

<svelte:head>
	<title>活动 - Bili Sync</title>
</svelte:head>

<div class="space-y-4">
	<!-- 工具栏：自动刷新开关 + 最后刷新时间 -->
	<div class="flex flex-wrap items-center justify-between gap-3">
		<div class="flex items-center gap-2">
			<Switch id="auto-refresh" checked={autoRefresh} onCheckedChange={handleAutoRefreshChange} />
			<Label for="auto-refresh" class="text-muted-foreground cursor-pointer text-xs font-normal">
				自动刷新（15 秒）
			</Label>
			{#if loadFailed}
				<span class="text-destructive text-xs">数据加载失败</span>
			{/if}
		</div>
		<div class="text-muted-foreground flex items-center gap-3 text-xs">
			<span>最后刷新 {formatTimeOfDay(lastRefresh)}</span>
			<Button
				variant="outline"
				size="sm"
				class="h-7 cursor-pointer px-2.5 text-xs"
				onclick={() => loadLists(false, true)}
				disabled={refreshing}
			>
				<RefreshCwIcon class="h-3 w-3 {refreshing ? 'animate-spin' : ''}" />
				刷新
			</Button>
		</div>
	</div>

	<!-- 下载任务状态 -->
	<Card>
		<CardHeader class="flex flex-row items-center justify-between space-y-0 pb-2">
			<CardTitle class="flex items-center gap-2 text-sm font-medium">
				下载任务
				<CloudDownloadIcon class="text-muted-foreground h-4 w-4" />
			</CardTitle>
		</CardHeader>
		<CardContent>
			{#if taskStatus}
				<div class="space-y-4">
					<div class="flex flex-wrap items-center justify-between gap-3">
						<div class="flex items-center gap-2">
							<span class="text-sm">当前状态</span>
							<Badge variant={taskStatus.is_running ? 'default' : 'outline'}>
								{taskStatus.is_running ? '运行中' : '未运行'}
							</Badge>
						</div>
						<Button
							size="sm"
							class="h-8 cursor-pointer text-xs"
							onclick={handleTriggerDownload}
							disabled={triggering || taskStatus.is_running}
						>
							<PlayIcon class="h-3.5 w-3.5" />
							{triggering ? '触发中...' : taskStatus.is_running ? '任务运行中' : '立即执行'}
						</Button>
					</div>
					<div class="grid gap-3 sm:grid-cols-3">
						<div class="rounded-md border p-2">
							<div class="text-muted-foreground flex items-center gap-1.5 text-xs">
								<PlayIcon class="h-3 w-3" />
								上次运行
							</div>
							<div class="mt-1 text-sm font-medium tabular-nums">
								{formatDateTime(taskStatus.last_run)}
							</div>
						</div>
						<div class="rounded-md border p-2">
							<div class="text-muted-foreground flex items-center gap-1.5 text-xs">
								<CircleCheckBigIcon class="h-3 w-3" />
								上次完成
							</div>
							<div class="mt-1 text-sm font-medium tabular-nums">
								{formatDateTime(taskStatus.last_finish)}
							</div>
						</div>
						<div class="rounded-md border p-2">
							<div class="text-muted-foreground flex items-center gap-1.5 text-xs">
								<CalendarIcon class="h-3 w-3" />
								下次运行
							</div>
							<div class="mt-1 text-sm font-medium tabular-nums">
								{formatDateTime(taskStatus.next_run)}
							</div>
						</div>
					</div>
				</div>
			{:else}
				<div class="text-muted-foreground py-6 text-center text-sm">等待任务状态推送...</div>
			{/if}
		</CardContent>
	</Card>

	<div class="grid gap-4 lg:grid-cols-2">
		<!-- 等待中 -->
		<Card>
			<CardHeader class="flex flex-row items-center justify-between space-y-0 pb-2">
				<CardTitle class="flex items-center gap-2 text-sm font-medium">
					等待中
					{#if !initialLoading}
						<Badge variant="secondary" class="font-normal">共 {waitingTotal} 条</Badge>
					{/if}
				</CardTitle>
				<a
					href="/videos?status_filter=waiting"
					class="text-primary flex items-center gap-0.5 text-xs hover:underline"
				>
					查看全部
					<ArrowRightIcon class="h-3 w-3" />
				</a>
			</CardHeader>
			<CardContent>
				{#if initialLoading}
					<div class="text-muted-foreground py-8 text-center text-sm">加载中...</div>
				{:else if waitingVideos.length === 0}
					<div class="text-muted-foreground py-8 text-center text-sm">暂无等待中的视频</div>
				{:else}
					<div class="divide-y">
						{#each waitingVideos as video (video.id)}
							<div class="flex items-center gap-3 py-2">
								<div class="min-w-0 flex-1">
									<a
										href={`/video/${video.id}`}
										class="line-clamp-1 text-sm leading-relaxed font-medium hover:underline"
										title={video.name}
									>
										{video.name}
									</a>
									<div class="text-muted-foreground mt-0.5 flex min-w-0 items-center gap-1 text-xs">
										<UserIcon class="h-3 w-3 shrink-0" />
										<span class="min-w-0 truncate" title={video.upper_name}>{video.upper_name}</span
										>
									</div>
								</div>
								<div
									class="flex w-24 shrink-0 flex-col items-end gap-1"
									title={getProgressTitle(video)}
								>
									<div class="flex w-full gap-0.5">
										{#each video.download_status as status, index (index)}
											<div class="h-1.5 flex-1 rounded-full {getSegmentColor(status)}"></div>
										{/each}
									</div>
									<span class="text-muted-foreground text-[10px] tabular-nums">
										{getCompletedCount(video.download_status)}/{video.download_status.length}
									</span>
								</div>
							</div>
						{/each}
					</div>
				{/if}
			</CardContent>
		</Card>

		<!-- 最近入库 -->
		<Card>
			<CardHeader class="flex flex-row items-center justify-between space-y-0 pb-2">
				<CardTitle class="flex items-center gap-2 text-sm font-medium">
					最近入库
					<ListVideoIcon class="text-muted-foreground h-4 w-4" />
				</CardTitle>
				<a href="/videos" class="text-primary flex items-center gap-0.5 text-xs hover:underline">
					查看全部
					<ArrowRightIcon class="h-3 w-3" />
				</a>
			</CardHeader>
			<CardContent>
				{#if initialLoading}
					<div class="text-muted-foreground py-8 text-center text-sm">加载中...</div>
				{:else if recentVideos.length === 0}
					<div class="text-muted-foreground py-8 text-center text-sm">暂无最近入库的视频</div>
				{:else}
					<div class="divide-y">
						{#each recentVideos as video (video.id)}
							{@const overallStatus = getOverallStatus(video)}
							<div class="flex items-center gap-3 py-2">
								<div class="min-w-0 flex-1">
									<a
										href={`/video/${video.id}`}
										class="line-clamp-1 text-sm leading-relaxed font-medium hover:underline"
										title={video.name}
									>
										{video.name}
									</a>
									<div class="text-muted-foreground mt-0.5 flex min-w-0 items-center gap-1 text-xs">
										<UserIcon class="h-3 w-3 shrink-0" />
										<span class="min-w-0 truncate" title={video.upper_name}>{video.upper_name}</span
										>
									</div>
								</div>
								<Badge
									variant="secondary"
									class="shrink-0 px-2 py-0.5 text-xs {overallStatus.style}"
								>
									{overallStatus.text}
								</Badge>
							</div>
						{/each}
					</div>
				{/if}
			</CardContent>
		</Card>
	</div>
</div>
