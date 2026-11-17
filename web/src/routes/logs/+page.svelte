<script lang="ts">
	import api from '$lib/api';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';
	import { onMount, tick } from 'svelte';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Switch } from '$lib/components/ui/switch/index.js';
	import { toast } from 'svelte-sonner';
	import CopyIcon from '@lucide/svelte/icons/copy';
	import SearchIcon from '@lucide/svelte/icons/search';
	import Trash2Icon from '@lucide/svelte/icons/trash-2';

	// 内存中最多保留的日志条数
	const MAX_LOGS = 500;
	// 级别过滤选项
	const LEVEL_OPTIONS = [
		{ value: 'ALL', label: '全部' },
		{ value: 'INFO', label: 'INFO' },
		{ value: 'WARN', label: 'WARN' },
		{ value: 'ERROR', label: 'ERROR' }
	] as const;

	type LogEntry = { timestamp: string; level: string; message: string };
	type LevelFilter = (typeof LEVEL_OPTIONS)[number]['value'];

	let unsubscribeLog: (() => void) | null = null;
	let logs: LogEntry[] = [];
	// 暂停期间先缓存新日志，恢复后再追加
	let pendingLogs: LogEntry[] = [];
	let levelFilter: LevelFilter = 'ALL';
	let keyword = '';
	let paused = false;
	let shouldAutoScroll = true;
	let main: HTMLElement | null = null;
	let scrollTimer: ReturnType<typeof setTimeout> | null = null;

	let normalizedKeyword = '';
	let filteredLogs: LogEntry[] = [];

	// 过滤依赖直接写在响应式语句里，确保 levelFilter / keyword 变化能触发重算
	$: normalizedKeyword = keyword.trim().toLowerCase();
	$: filteredLogs = logs.filter((log) => {
		if (levelFilter !== 'ALL' && log.level !== levelFilter) {
			return false;
		}
		return normalizedKeyword === '' || log.message.toLowerCase().includes(normalizedKeyword);
	});

	function checkScrollPosition() {
		if (main) {
			const { scrollTop, scrollHeight, clientHeight } = main;
			shouldAutoScroll = scrollTop + clientHeight >= scrollHeight - 50;
		}
	}

	async function scrollToBottom() {
		await tick();
		if (shouldAutoScroll && main) {
			main.scrollTop = main.scrollHeight;
		}
	}

	function scheduleScroll() {
		if (scrollTimer) clearTimeout(scrollTimer);
		scrollTimer = setTimeout(scrollToBottom, 20);
	}

	function appendLog(log: LogEntry) {
		if (paused) {
			pendingLogs = [...pendingLogs, log].slice(-MAX_LOGS);
			return;
		}
		logs = [...logs.slice(-(MAX_LOGS - 1)), log];
		scheduleScroll();
	}

	// 暂停恢复后补上暂停期间未显示的日志
	function handlePauseChange(next: boolean) {
		paused = next;
		if (!next && pendingLogs.length > 0) {
			logs = [...logs, ...pendingLogs].slice(-MAX_LOGS);
			pendingLogs = [];
			scheduleScroll();
		}
	}

	function handleClear() {
		logs = [];
		pendingLogs = [];
		toast.info('日志已清空');
	}

	function fallbackCopy(text: string) {
		const textarea = document.createElement('textarea');
		textarea.value = text;
		textarea.style.position = 'fixed';
		textarea.style.opacity = '0';
		document.body.appendChild(textarea);
		textarea.select();
		// 局域网 HTTP 部署时 navigator.clipboard 不可用，只能退回已废弃的 execCommand
		// eslint-disable-next-line @typescript-eslint/no-deprecated
		const copied = document.execCommand('copy');
		document.body.removeChild(textarea);
		if (!copied) {
			throw new Error('当前浏览器不支持自动复制');
		}
	}

	async function handleCopy() {
		if (filteredLogs.length === 0) {
			toast.info('当前没有可复制的日志');
			return;
		}
		const text = filteredLogs
			.map((log) => `${log.timestamp} [${log.level}] ${log.message}`)
			.join('\n');
		try {
			if (navigator.clipboard?.writeText) {
				await navigator.clipboard.writeText(text);
			} else {
				fallbackCopy(text);
			}
			toast.success(`已复制 ${filteredLogs.length} 条日志`);
		} catch (error) {
			console.error('复制日志失败：', error);
			toast.error('复制日志失败', {
				description: error instanceof Error ? error.message : String(error)
			});
		}
	}

	onMount(() => {
		setBreadcrumb([{ label: '日志' }]);
		main = document.getElementById('main');
		main?.addEventListener('scroll', checkScrollPosition);
		unsubscribeLog = api.subscribeToLogs((data: string) => {
			let log: LogEntry;
			try {
				log = JSON.parse(data) as LogEntry;
			} catch (error) {
				console.error('解析日志失败：', error);
				return;
			}
			appendLog(log);
		});
		return () => {
			if (scrollTimer) clearTimeout(scrollTimer);
			main?.removeEventListener('scroll', checkScrollPosition);
			if (unsubscribeLog) {
				unsubscribeLog();
				unsubscribeLog = null;
			}
		};
	});

	function getLevelColor(level: string) {
		switch (level) {
			case 'ERROR':
				return 'text-rose-600';
			case 'WARN':
				return 'text-yellow-600';
			case 'INFO':
			default:
				return 'text-emerald-600';
		}
	}
</script>

<svelte:head>
	<title>日志 - Bili Sync</title>
</svelte:head>

<div class="space-y-2">
	<!-- 工具栏：级别过滤 / 关键字搜索 / 暂停 / 清空 / 复制 -->
	<div
		class="bg-background sticky top-0 z-10 flex flex-wrap items-center gap-x-4 gap-y-2 border-b py-2"
	>
		<div class="flex items-center gap-1">
			{#each LEVEL_OPTIONS as option (option.value)}
				<Button
					size="sm"
					variant={levelFilter === option.value ? 'default' : 'outline'}
					class="h-7 cursor-pointer px-2.5 text-xs"
					onclick={() => (levelFilter = option.value)}
				>
					{option.label}
				</Button>
			{/each}
		</div>

		<div class="relative">
			<SearchIcon
				class="text-muted-foreground pointer-events-none absolute top-1/2 left-2 h-3.5 w-3.5 -translate-y-1/2"
			/>
			<Input
				class="h-8 w-56 pl-7 text-xs"
				placeholder="搜索日志内容..."
				aria-label="搜索日志内容"
				bind:value={keyword}
			/>
		</div>

		<div class="flex items-center gap-2">
			<Switch id="pause-logs" checked={paused} onCheckedChange={handlePauseChange} />
			<Label for="pause-logs" class="text-muted-foreground cursor-pointer text-xs font-normal">
				{paused ? '已暂停' : '暂停'}
			</Label>
			{#if paused && pendingLogs.length > 0}
				<span class="text-muted-foreground text-xs">待补 {pendingLogs.length} 条</span>
			{/if}
		</div>

		<div class="ml-auto flex items-center gap-2">
			<span class="text-muted-foreground text-xs tabular-nums">
				筛选后 {filteredLogs.length} 条 / 共 {logs.length} 条
			</span>
			<Button
				size="sm"
				variant="outline"
				class="h-8 cursor-pointer px-2.5 text-xs"
				onclick={handleClear}
				disabled={logs.length === 0 && pendingLogs.length === 0}
			>
				<Trash2Icon class="h-3.5 w-3.5" />
				清空
			</Button>
			<Button
				size="sm"
				variant="outline"
				class="h-8 cursor-pointer px-2.5 text-xs"
				onclick={handleCopy}
				disabled={filteredLogs.length === 0}
			>
				<CopyIcon class="h-3.5 w-3.5" />
				复制结果
			</Button>
		</div>
	</div>

	<div class="space-y-1">
		{#each filteredLogs as log, index (index)}
			<div
				class="flex items-center gap-3 rounded-md p-1 font-mono text-xs {index % 2 === 0
					? 'bg-muted/50'
					: 'bg-background'}"
			>
				<span class="text-muted-foreground w-32 shrink-0">
					{log.timestamp}
				</span>
				<Badge
					class="w-16 shrink-0 justify-center {getLevelColor(
						log.level
					)} bg-primary/90 font-semibold"
				>
					{log.level}
				</Badge>
				<span class="flex-1 break-all">
					{log.message}
				</span>
			</div>
		{/each}
		{#if filteredLogs.length === 0}
			<div class="text-muted-foreground py-8 text-center">
				{logs.length === 0 ? '暂无日志记录' : '没有符合筛选条件的日志'}
			</div>
		{/if}
	</div>
</div>
