<script lang="ts">
	import { onDestroy, onMount, tick } from 'svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Switch } from '$lib/components/ui/switch/index.js';
	import { Separator } from '$lib/components/ui/separator/index.js';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import * as Tooltip from '$lib/components/ui/tooltip/index.js';
	import PasswordInput from '$lib/components/custom/password-input.svelte';
	import QrLogin from '$lib/components/custom/qr-login.svelte';
	import FilterOptionEditor from '$lib/components/filter-option-editor.svelte';
	import NotifierDialog from './NotifierDialog.svelte';
	import InfoIcon from '@lucide/svelte/icons/info';
	import QrCodeIcon from '@lucide/svelte/icons/qr-code';
	import api from '$lib/api';
	import { toast } from 'svelte-sonner';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';
	import type { Config, ApiError, Notifier, Credential, DanmakuUpdateMilestone } from '$lib/types';

	let frontendToken = ''; // 前端认证token
	let config: Config | null = null;
	let formData: Config | null = null;
	let saving = false;
	let loading = false;

	let intervalInput: string = '1200';

	// 配置区块目录，id 与页面中的 section 一一对应
	const sectionList = [
		{ id: 'section-basic', label: '基础设置' },
		{ id: 'section-paths', label: '存储与路径' },
		{ id: 'section-security', label: '接口认证' },
		{ id: 'section-auth', label: 'B 站账号' },
		{ id: 'section-filter', label: '视频过滤' },
		{ id: 'section-processing', label: '跳过项与 NFO' },
		{ id: 'section-danmaku', label: '弹幕设置' },
		{ id: 'section-notifiers', label: '通知器' },
		{ id: 'section-download', label: '下载与并发' }
	];

	let activeSection = sectionList[0].id;
	let sectionObserver: IntersectionObserver | null = null;

	// 未保存状态：加载/保存成功后记录表单快照，和当前表单比较
	let snapshot = '';
	let formContent: HTMLElement | null = null;
	// 部分改动只发生在表单对象内部（each 项、子组件），不会重新赋值 formData，
	// 用内容区冒泡事件累加版本号，让下面的响应式语句重新计算快照
	let changeVersion = 0;

	$: currentSnapshot = buildSnapshot(formData, intervalInput, changeVersion);
	$: dirty = Boolean(formData) && snapshot !== currentSnapshot;

	function buildSnapshot(config: Config | null, interval: string, version: number) {
		void version; // 版本号只用于触发重算，不参与快照比较
		return JSON.stringify({ config, interval });
	}

	function takeSnapshot() {
		snapshot = buildSnapshot(formData, intervalInput, changeVersion);
	}

	function handleFormMutation() {
		changeVersion += 1;
	}

	// 显式把当前高亮项作为参数传入：模板里直接引用 activeSection 才能触发重渲染
	function navItemClass(id: string, current: string) {
		return current === id
			? 'bg-muted text-foreground font-medium'
			: 'text-muted-foreground hover:bg-muted/60 hover:text-foreground';
	}

	function navChipClass(id: string, current: string) {
		return current === id
			? 'bg-primary text-primary-foreground border-transparent'
			: 'bg-background text-muted-foreground hover:bg-muted';
	}

	function scrollToSection(id: string) {
		activeSection = id;
		document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
	}

	// 根据各区块相对滚动容器顶部的位置计算当前高亮项
	function updateActiveSection() {
		const elements = sectionList
			.map((section) => document.getElementById(section.id))
			.filter((element): element is HTMLElement => element !== null);
		if (elements.length === 0) return;

		const root = document.getElementById('main');
		const rootTop = root ? root.getBoundingClientRect().top : 0;
		const activationLine = rootTop + 120;
		let current = elements[0].id;
		for (const element of elements) {
			if (element.getBoundingClientRect().top <= activationLine) {
				current = element.id;
			} else {
				break;
			}
		}
		// 滚到底部时最后一个区块不一定能越过激活线，这里兜底
		if (root && root.scrollTop + root.clientHeight >= root.scrollHeight - 8) {
			current = elements[elements.length - 1].id;
		}
		activeSection = current;
	}

	async function initSectionObserver() {
		await tick();
		sectionObserver?.disconnect();
		sectionObserver = null;

		const elements = sectionList
			.map((section) => document.getElementById(section.id))
			.filter((element): element is HTMLElement => element !== null);
		if (elements.length === 0) return;

		// 本应用的主滚动区是布局里的 div#main，而不是 window
		const root = document.getElementById('main');
		sectionObserver = new IntersectionObserver(() => updateActiveSection(), {
			root,
			rootMargin: '-120px 0px -55% 0px',
			threshold: 0
		});
		elements.forEach((element) => sectionObserver?.observe(element));
		updateActiveSection();

		// 子组件（如过滤规则里的开关）的改动不一定重新赋值 formData，用原生监听兜底
		formContent?.removeEventListener('click', handleFormMutation);
		formContent?.addEventListener('click', handleFormMutation);
	}

	onDestroy(() => {
		sectionObserver?.disconnect();
		formContent?.removeEventListener('click', handleFormMutation);
	});

	// Notifier 管理相关
	let showNotifierDialog = false;
	let editingNotifier: Notifier | null = null;
	let editingNotifierIndex: number | null = null;
	let isEditing = false;

	// QR 登录 Dialog 相关
	let showQrLoginDialog = false;
	let qrLoginComponent: QrLogin;

	function openAddNotifierDialog() {
		editingNotifier = null;
		editingNotifierIndex = null;
		isEditing = false;
		showNotifierDialog = true;
	}

	function openEditNotifierDialog(notifier: Notifier, index: number) {
		editingNotifier = { ...notifier };
		editingNotifierIndex = index;
		isEditing = true;
		showNotifierDialog = true;
	}

	function closeNotifierDialog() {
		showNotifierDialog = false;
		editingNotifier = null;
		editingNotifierIndex = null;
		isEditing = false;
	}

	function addNotifier(notifier: Notifier) {
		if (!formData) return;
		if (!formData.notifiers) {
			formData.notifiers = [];
		}
		formData.notifiers = [...formData.notifiers, notifier];
		closeNotifierDialog();
	}

	function updateNotifier(index: number, notifier: Notifier) {
		if (!formData?.notifiers) return;
		const newNotifiers = [...formData.notifiers];
		newNotifiers[index] = notifier;
		formData.notifiers = newNotifiers;
		closeNotifierDialog();
	}

	function deleteNotifier(index: number) {
		if (!formData?.notifiers) return;
		formData.notifiers = formData.notifiers.filter((_, i) => i !== index);
	}

	async function testNotifier(notifier: Notifier) {
		try {
			await api.testNotifier(notifier);
			toast.success('测试通知发送成功');
		} catch (error) {
			console.error('测试通知失败:', error);
			toast.error('测试通知失败', {
				description: (error as ApiError).message
			});
		}
	}

	async function loadConfig() {
		loading = true;
		try {
			const response = await api.getConfig();
			config = response.data;
			formData = { ...config };

			// 根据 interval 的类型初始化输入框
			if (typeof formData.interval === 'number') {
				intervalInput = String(formData.interval);
			} else {
				intervalInput = formData.interval;
			}

			takeSnapshot();
		} catch (error) {
			console.error('加载配置失败:', error);
			toast.error('加载配置失败', {
				description: (error as ApiError).message
			});
			throw error;
		} finally {
			loading = false;
		}

		// 等区块渲染进 DOM 后再建立滚动高亮观察器
		void initSectionObserver();
	}

	async function authenticateFrontend() {
		if (!frontendToken.trim()) {
			toast.error('请输入前端认证Token');
			return;
		}

		try {
			api.setAuthToken(frontendToken.trim());
			await loadConfig();
			toast.success('前端认证成功');
		} catch (error) {
			console.error('前端认证失败:', error);
			toast.error('认证失败，请检查Token是否正确', {
				description: (error as ApiError).message
			});
		}
	}

	async function saveConfig() {
		if (!formData) {
			toast.error('配置未加载');
			return;
		}

		// 保存前根据输入内容判断类型
		const trimmed = intervalInput.trim();
		const asNumber = Number(trimmed);

		if (!isNaN(asNumber) && trimmed !== '') {
			// 纯数字，作为 Interval
			formData.interval = asNumber;
		} else {
			// 非数字，作为 Cron 表达式
			formData.interval = trimmed;
		}

		saving = true;
		try {
			let resp = await api.updateConfig(formData);
			formData = resp.data;
			config = { ...formData };

			// 更新输入框显示
			if (typeof formData.interval === 'number') {
				intervalInput = String(formData.interval);
			} else {
				intervalInput = formData.interval;
			}

			takeSnapshot();
			toast.success('配置已保存');
		} catch (error) {
			console.error('保存配置失败:', error);
			toast.error('保存配置失败', {
				description: (error as ApiError).message
			});
		} finally {
			saving = false;
		}
	}

	function addDanmakuUpdateMilestone() {
		if (!formData) return;
		const milestones = formData.danmaku_update_policy.milestones;
		const last = milestones[milestones.length - 1];
		formData.danmaku_update_policy.milestones = [
			...milestones,
			{
				type: 'once',
				at_days: last ? danmakuUpdateMilestoneEndDays(last) + 30 : 3
			}
		];
	}

	function danmakuUpdateMilestoneEndDays(milestone: DanmakuUpdateMilestone) {
		return milestone.type === 'once' ? milestone.at_days : milestone.until_days;
	}

	function changeDanmakuUpdateMilestoneType(index: number, type: 'once' | 'periodic') {
		if (!formData) return;
		const milestones = [...formData.danmaku_update_policy.milestones];
		const current = milestones[index];
		const endDays = danmakuUpdateMilestoneEndDays(current);
		milestones[index] =
			type === 'once'
				? { type, at_days: endDays }
				: { type, until_days: endDays, interval_hours: 24 };
		formData.danmaku_update_policy.milestones = milestones;
	}

	function removeDanmakuUpdateMilestone(index: number) {
		if (!formData) return;
		formData.danmaku_update_policy.milestones = formData.danmaku_update_policy.milestones.filter(
			(_, milestoneIndex) => milestoneIndex !== index
		);
	}

	function handleQrLoginSuccess(credential: Credential) {
		if (!formData) return;

		// 自动填充凭证到 formData
		formData.credential = credential;

		toast.success('扫码登录成功，已填充凭据');

		// 自动保存配置
		saveConfig();

		// 关闭弹窗
		showQrLoginDialog = false;
	}

	onMount(() => {
		setBreadcrumb([{ label: '设置' }]);
		frontendToken = api.getAuthToken() || '';
		loadConfig();
	});
</script>

<svelte:head>
	<title>设置 - Bili Sync</title>
</svelte:head>

<div class="space-y-6">
	<!-- 前端认证状态栏 -->
	<div class="bg-card rounded-lg border p-4">
		<div class="flex items-center justify-between">
			<div class="space-y-1">
				<h2 class="font-semibold">前端认证状态</h2>
				<p class="text-muted-foreground text-sm">
					{formData ? '已认证 - 可以正常加载数据' : '未认证 - 请输入 Token 进行鉴权'}
				</p>
			</div>
			{#if !formData}
				<div class="flex gap-3">
					<PasswordInput bind:value={frontendToken} placeholder="输入认证Token" />
					<Button onclick={authenticateFrontend} disabled={!frontendToken.trim()}>认证</Button>
				</div>
			{:else}
				<div class="flex items-center gap-3">
					<div class="flex items-center gap-2">
						<div class="h-2 w-2 rounded-full bg-emerald-500"></div>
						<span class="text-sm text-emerald-600">已认证</span>
					</div>
					<Button
						variant="outline"
						size="sm"
						onclick={() => {
							formData = null;
							config = null;
							api.clearAuthToken();
							frontendToken = '';
							snapshot = '';
							sectionObserver?.disconnect();
							sectionObserver = null;
						}}
					>
						退出认证
					</Button>
				</div>
			{/if}
		</div>
	</div>

	<!-- 应用配置 -->
	{#if loading}
		<div class="flex items-center justify-center py-16">
			<div class="space-y-2 text-center">
				<div
					class="border-primary mx-auto h-6 w-6 animate-spin rounded-full border-2 border-t-transparent"
				></div>
				<p class="text-muted-foreground">加载配置中...</p>
			</div>
		</div>
	{:else if formData}
		<div class="lg:grid lg:grid-cols-[12rem_minmax(0,1fr)] lg:gap-8">
			<!-- 配置目录：桌面端左侧吸顶，移动端顶部横向可滚动 chips -->
			<aside class="min-w-0 self-start lg:sticky lg:top-4">
				<div class="no-scrollbar -mx-1 flex gap-2 overflow-x-auto px-1 pb-1 lg:hidden">
					{#each sectionList as section (section.id)}
						<button
							type="button"
							class="shrink-0 rounded-full border px-3 py-1 text-xs whitespace-nowrap transition-colors {navChipClass(
								section.id,
								activeSection
							)}"
							onclick={() => scrollToSection(section.id)}
						>
							{section.label}
						</button>
					{/each}
				</div>

				<nav class="hidden lg:block" aria-label="配置目录">
					<p class="text-muted-foreground mb-2 px-3 text-xs font-medium tracking-wide">配置目录</p>
					<ul class="space-y-1 text-sm">
						{#each sectionList as section (section.id)}
							<li>
								<button
									type="button"
									class="w-full rounded-md px-3 py-1.5 text-left transition-colors {navItemClass(
										section.id,
										activeSection
									)}"
									onclick={() => scrollToSection(section.id)}
								>
									{section.label}
								</button>
							</li>
						{/each}
					</ul>
				</nav>
			</aside>

			<!-- 配置内容：各区块按目录顺序排布，id 与目录一一对应 -->
			<div
				class="min-w-0 space-y-6"
				bind:this={formContent}
				oninput={handleFormMutation}
				onchange={handleFormMutation}
			>
				<!-- 基础设置 -->
				<section id="section-basic" class="bg-card scroll-mt-4 rounded-lg border p-5">
					<h2 class="text-base font-semibold">基础设置</h2>
					<p class="text-muted-foreground mt-1 mb-4 text-sm">
						服务监听地址、任务触发条件与下载策略开关
					</p>

					<div class="space-y-6">
						<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
							<div class="space-y-2">
								<Label for="bind-address">绑定地址</Label>
								<Input
									id="bind-address"
									placeholder="127.0.0.1:9999"
									bind:value={formData.bind_address}
								/>
							</div>
							<div class="space-y-2">
								<div class="flex items-center gap-1">
									<Label for="interval">任务触发条件</Label>
									<Tooltip.Root>
										<Tooltip.Trigger>
											<InfoIcon class="text-muted-foreground h-3.5 w-3.5" />
										</Tooltip.Trigger>
										<Tooltip.Content>
											<p class="text-xs">
												视频下载任务的触发条件，支持两种格式：<br />
												1. 输入数字表示间隔秒数，例如 1200 表示每隔 20 分钟触发一次； <br />
												2. 输入 Cron 表达式，格式为“秒 分 时 日 月 周”，例如“0 0 2 * * *”表示每天凌晨2点触发一次。
											</p>
										</Tooltip.Content>
									</Tooltip.Root>
								</div>
								<Input
									id="interval"
									type="text"
									bind:value={intervalInput}
									placeholder="1200 或 0 0 2 * * *"
								/>
							</div>
						</div>

						<Separator />

						<div class="space-y-4">
							<div class="flex items-center space-x-2">
								<Switch id="cdn-sorting" bind:checked={formData.cdn_sorting} />
								<Label for="cdn-sorting">启用CDN排序</Label>
							</div>
							<div class="flex items-center space-x-2">
								<Switch id="try-upower-anyway" bind:checked={formData.try_upower_anyway} />
								<div class="flex items-center gap-1">
									<Label for="try-upower-anyway">尝试下载未充电视频</Label>
									<Tooltip.Root>
										<Tooltip.Trigger>
											<InfoIcon class="text-muted-foreground h-3.5 w-3.5" />
										</Tooltip.Trigger>
										<Tooltip.Content>
											<p class="text-xs">
												当关闭该开关时，程序仅会下载已充电的视频，未充电的视频直接跳过；开启后不再检查充电状态，一律尝试下载。<br
												/>
												这可以帮助下载未充电视频的封面等元数据，也应该可以下载未充电视频的试看部分（如果存在的话）。
											</p>
										</Tooltip.Content>
									</Tooltip.Root>
								</div>
							</div>
						</div>
					</div>
				</section>

				<!-- 存储与路径 -->
				<section id="section-paths" class="bg-card scroll-mt-4 rounded-lg border p-5">
					<h2 class="text-base font-semibold">存储与路径</h2>
					<p class="text-muted-foreground mt-1 mb-4 text-sm">
						文件命名模板、UP 主信息存放位置与快捷订阅的默认路径
					</p>

					<div class="space-y-6">
						<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
							<div class="space-y-2">
								<Label for="video-name">视频名称模板</Label>
								<Input id="video-name" bind:value={formData.video_name} />
							</div>
							<div class="space-y-2">
								<Label for="page-name">分页名称模板</Label>
								<Input id="page-name" bind:value={formData.page_name} />
							</div>
							<div class="space-y-2">
								<Label for="upper-path">UP主头像保存路径</Label>
								<Input
									id="upper-path"
									placeholder="/path/to/upper/faces"
									bind:value={formData.upper_path}
								/>
							</div>
							<div class="space-y-2">
								<Label for="time-format">时间格式</Label>
								<Input
									id="time-format"
									placeholder="%Y-%m-%d %H:%M:%S"
									bind:value={formData.time_format}
								/>
							</div>
						</div>

						<Separator />

						<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
							<div class="space-y-2">
								<Label for="favorite-default-path">收藏夹快捷订阅路径模板</Label>
								<Input id="favorite-default-path" bind:value={formData.favorite_default_path} />
							</div>
							<div class="space-y-2">
								<Label for="collection-default-path">合集快捷订阅路径模板</Label>
								<Input id="collection-default-path" bind:value={formData.collection_default_path} />
							</div>
							<div class="space-y-2">
								<Label for="submission-default-path">UP 主投稿快捷订阅路径模板</Label>
								<Input id="submission-default-path" bind:value={formData.submission_default_path} />
							</div>
						</div>
					</div>
				</section>

				<!-- 接口认证 -->
				<section id="section-security" class="bg-card scroll-mt-4 rounded-lg border p-5">
					<h2 class="text-base font-semibold">接口认证</h2>
					<p class="text-muted-foreground mt-1 mb-4 text-sm">后端 API 的访问认证</p>

					<div class="space-y-2">
						<Label for="backend-auth-token">后端 API 认证Token</Label>
						<PasswordInput
							id="backend-auth-token"
							placeholder="设置后端 API 认证Token"
							bind:value={formData.auth_token}
						/>
						<p class="text-muted-foreground text-xs">
							修改此Token后，前端需要使用新Token重新认证才能访问API
						</p>
					</div>
				</section>

				<!-- B站认证 -->
				<section id="section-auth" class="bg-card scroll-mt-4 rounded-lg border p-5">
					<h2 class="text-base font-semibold">B 站账号</h2>
					<p class="text-muted-foreground mt-1 mb-4 text-sm">
						扫码登录或手动填写哔哩哔哩 Cookie 凭据
					</p>

					<div class="space-y-6">
						<div class="flex items-center justify-between">
							<div class="space-y-1">
								<Label class="text-base font-semibold">快速登录</Label>
								<p class="text-muted-foreground text-sm">使用哔哩哔哩 APP 扫码登录，自动填充凭据</p>
							</div>
							<Button
								onclick={() => {
									showQrLoginDialog = true;
									tick().then(() => {
										qrLoginComponent!.init();
									});
								}}
							>
								<QrCodeIcon class="mr-2 h-4 w-4" />
								扫码登录
							</Button>
						</div>

						<Separator />

						<!-- 原有的手动输入 Cookie 表单 -->
						<div class="space-y-4">
							<div class="space-y-2">
								<Label for="sessdata">SESSDATA</Label>
								<PasswordInput
									id="sessdata"
									placeholder="请输入SESSDATA"
									bind:value={formData.credential.sessdata}
								/>
							</div>
							<div class="space-y-2">
								<Label for="bili-jct">bili_jct</Label>
								<PasswordInput
									id="bili-jct"
									placeholder="请输入bili_jct"
									bind:value={formData.credential.bili_jct}
								/>
							</div>
							<div class="space-y-2">
								<Label for="buvid3">buvid3</Label>
								<PasswordInput
									id="buvid3"
									placeholder="请输入buvid3"
									bind:value={formData.credential.buvid3}
								/>
							</div>
							<div class="space-y-2">
								<Label for="dedeuserid">dedeuserid</Label>
								<PasswordInput
									id="dedeuserid"
									placeholder="请输入dedeuserid"
									bind:value={formData.credential.dedeuserid}
								/>
							</div>
							<div class="space-y-2">
								<Label for="ac-time-value">ac_time_value</Label>
								<PasswordInput
									id="ac-time-value"
									placeholder="请输入ac_time_value"
									bind:value={formData.credential.ac_time_value}
								/>
							</div>
						</div>
					</div>
				</section>

				<!-- 视频过滤 -->
				<section id="section-filter" class="bg-card scroll-mt-4 rounded-lg border p-5">
					<h2 class="text-base font-semibold">视频过滤</h2>
					<p class="text-muted-foreground mt-1 mb-4 text-sm">配置视频过滤规则</p>

					<FilterOptionEditor bind:value={formData.filter_option} />
				</section>

				<!-- 跳过项与 NFO -->
				<section id="section-processing" class="bg-card scroll-mt-4 rounded-lg border p-5">
					<h2 class="text-base font-semibold">跳过项与 NFO</h2>
					<p class="text-muted-foreground mt-1 mb-4 text-sm">
						视频处理过程中可以跳过的环节与 NFO 时间来源
					</p>

					<div class="space-y-6">
						<div class="space-y-4">
							<Label>处理跳过选项</Label>
							<p class="text-muted-foreground text-sm">在视频处理部分跳过某些执行环节</p>
							<div class="flex items-center space-x-2">
								<Switch id="skip-poster" bind:checked={formData.skip_option.no_poster} />
								<Label for="skip-poster">跳过视频封面</Label>
							</div>
							<div class="flex items-center space-x-2">
								<Switch id="skip-video-nfo" bind:checked={formData.skip_option.no_video_nfo} />
								<Label for="skip-video-nfo">跳过视频 NFO</Label>
							</div>
							<div class="flex items-center space-x-2">
								<Switch id="skip-upper-info" bind:checked={formData.skip_option.no_upper} />
								<Label for="skip-upper-info">跳过 Up 主头像、信息</Label>
							</div>
							<div class="flex items-center space-x-2">
								<Switch id="skip-danmaku" bind:checked={formData.skip_option.no_danmaku} />
								<Label for="skip-danmaku">跳过弹幕</Label>
							</div>
							<div class="flex items-center space-x-2">
								<Switch id="skip-subtitle" bind:checked={formData.skip_option.no_subtitle} />
								<Label for="skip-subtitle">跳过字幕</Label>
							</div>
						</div>

						<Separator />

						<div class="max-w-md space-y-2">
							<Label for="nfo-time-type">NFO时间类型</Label>
							<select
								id="nfo-time-type"
								class="border-input bg-background ring-offset-background placeholder:text-muted-foreground focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm file:border-0 file:bg-transparent file:text-sm file:font-medium focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
								bind:value={formData.nfo_time_type}
							>
								<option value="favtime">收藏时间</option>
								<option value="pubtime">发布时间</option>
							</select>
						</div>
					</div>
				</section>

				<!-- 弹幕设置 -->
				<section id="section-danmaku" class="bg-card scroll-mt-4 rounded-lg border p-5">
					<h2 class="text-base font-semibold">弹幕设置</h2>
					<p class="text-muted-foreground mt-1 mb-4 text-sm">弹幕自动更新策略与弹幕渲染参数</p>

					<div class="space-y-6">
						<div class="space-y-4">
							<div class="flex items-center justify-between">
								<div class="space-y-1">
									<Label for="danmaku-update-enabled">弹幕自动更新</Label>
									<p class="text-muted-foreground text-sm">
										例如：希望第 3 天更新一次，之后每 7 天更新一次直到第 30 天，可添加“单次：3
										天”和“周期：截止 30 天，间隔 168 小时”。所有时间均从视频发布时间起算
									</p>
								</div>
								<Switch
									id="danmaku-update-enabled"
									bind:checked={formData.danmaku_update_policy.enabled}
								/>
							</div>

							<div class="space-y-3">
								{#each formData.danmaku_update_policy.milestones as milestone, index (index)}
									<div class="grid grid-cols-1 items-end gap-3 md:grid-cols-[9rem_1fr_1fr_auto]">
										<div class="space-y-2">
											<Label for={`danmaku-milestone-type-${index}`}>类型</Label>
											<select
												id={`danmaku-milestone-type-${index}`}
												class="border-input bg-background ring-offset-background focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
												disabled={!formData.danmaku_update_policy.enabled}
												value={milestone.type}
												onchange={(event) =>
													changeDanmakuUpdateMilestoneType(
														index,
														event.currentTarget.value as 'once' | 'periodic'
													)}
											>
												<option value="once">单次</option>
												<option value="periodic">周期</option>
											</select>
										</div>
										{#if milestone.type === 'once'}
											<div class="space-y-2 md:col-span-2">
												<Label for={`danmaku-milestone-at-${index}`}>发布时间后第几天</Label>
												<Input
													id={`danmaku-milestone-at-${index}`}
													type="number"
													min="1"
													disabled={!formData.danmaku_update_policy.enabled}
													bind:value={milestone.at_days}
												/>
											</div>
										{:else}
											<div class="space-y-2">
												<Label for={`danmaku-milestone-until-${index}`}>截止天数</Label>
												<Input
													id={`danmaku-milestone-until-${index}`}
													type="number"
													min="1"
													disabled={!formData.danmaku_update_policy.enabled}
													bind:value={milestone.until_days}
												/>
											</div>
											<div class="space-y-2">
												<Label for={`danmaku-milestone-interval-${index}`}>刷新间隔（小时）</Label>
												<Input
													id={`danmaku-milestone-interval-${index}`}
													type="number"
													min="1"
													disabled={!formData.danmaku_update_policy.enabled}
													bind:value={milestone.interval_hours}
												/>
											</div>
										{/if}
										<Button
											variant="outline"
											disabled={!formData.danmaku_update_policy.enabled}
											onclick={() => removeDanmakuUpdateMilestone(index)}>删除</Button
										>
									</div>
								{/each}
								<Button
									variant="outline"
									disabled={!formData.danmaku_update_policy.enabled}
									onclick={addDanmakuUpdateMilestone}>添加里程碑</Button
								>
							</div>
						</div>

						<Separator />

						<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
							<div class="space-y-2">
								<Label for="danmaku-duration">弹幕持续时间（秒）</Label>
								<Input
									id="danmaku-duration"
									type="number"
									min="1"
									step="0.1"
									bind:value={formData.danmaku_option.duration}
								/>
							</div>
							<div class="space-y-2">
								<Label for="danmaku-font">字体</Label>
								<Input
									id="danmaku-font"
									placeholder="黑体"
									bind:value={formData.danmaku_option.font}
								/>
							</div>
							<div class="space-y-2">
								<Label for="danmaku-font-size">字体大小</Label>
								<Input
									id="danmaku-font-size"
									type="number"
									min="8"
									max="72"
									bind:value={formData.danmaku_option.font_size}
								/>
							</div>
							<div class="space-y-2">
								<Label for="danmaku-width-ratio">宽度比例</Label>
								<Input
									id="danmaku-width-ratio"
									type="number"
									min="0.1"
									max="3"
									step="0.1"
									bind:value={formData.danmaku_option.width_ratio}
								/>
							</div>
							<div class="space-y-2">
								<Label for="danmaku-horizontal-gap">水平间距</Label>
								<Input
									id="danmaku-horizontal-gap"
									type="number"
									min="0"
									step="1"
									bind:value={formData.danmaku_option.horizontal_gap}
								/>
							</div>
							<div class="space-y-2">
								<Label for="danmaku-lane-size">轨道大小</Label>
								<Input
									id="danmaku-lane-size"
									type="number"
									min="8"
									max="128"
									bind:value={formData.danmaku_option.lane_size}
								/>
							</div>
							<div class="space-y-2">
								<Label for="danmaku-float-percentage">滚动弹幕高度百分比</Label>
								<Input
									id="danmaku-float-percentage"
									type="number"
									min="0"
									max="1"
									step="0.01"
									bind:value={formData.danmaku_option.float_percentage}
								/>
							</div>
							<div class="space-y-2">
								<Label for="danmaku-bottom-percentage">底部弹幕高度百分比</Label>
								<Input
									id="danmaku-bottom-percentage"
									type="number"
									min="0"
									max="1"
									step="0.01"
									bind:value={formData.danmaku_option.bottom_percentage}
								/>
							</div>
							<div class="space-y-2">
								<Label for="danmaku-opacity">透明度 (0-255)</Label>
								<Input
									id="danmaku-opacity"
									type="number"
									min="0"
									max="255"
									bind:value={formData.danmaku_option.opacity}
								/>
							</div>
							<div class="space-y-2">
								<Label for="danmaku-outline">描边宽度</Label>
								<Input
									id="danmaku-outline"
									type="number"
									min="0"
									max="5"
									step="0.1"
									bind:value={formData.danmaku_option.outline}
								/>
							</div>
							<div class="space-y-2">
								<Label for="danmaku-time-offset">时间偏移（秒）</Label>
								<Input
									id="danmaku-time-offset"
									type="number"
									step="0.1"
									bind:value={formData.danmaku_option.time_offset}
								/>
							</div>
						</div>

						<Separator />

						<div class="flex items-center space-x-2">
							<Switch id="danmaku-bold" bind:checked={formData.danmaku_option.bold} />
							<Label for="danmaku-bold">粗体显示</Label>
						</div>
					</div>
				</section>

				<!-- 通知器 -->
				<section id="section-notifiers" class="bg-card scroll-mt-4 rounded-lg border p-5">
					<h2 class="text-base font-semibold">通知器</h2>
					<p class="text-muted-foreground mt-1 mb-4 text-sm">下载任务出现错误时发送通知</p>

					<div class="space-y-6">
						<div class="space-y-4">
							<div class="flex items-center justify-between">
								<div>
									<h3 class="text-lg font-semibold">通知器管理</h3>
									<p class="text-muted-foreground text-sm">
										配置通知器，在下载任务出现错误时发送通知
									</p>
								</div>
								<Button onclick={openAddNotifierDialog}>+ 添加通知器</Button>
							</div>

							{#if !formData.notifiers || formData.notifiers.length === 0}
								<div class="rounded-lg border-2 border-dashed py-12 text-center">
									<p class="text-muted-foreground">暂无通知器配置</p>
									<Button class="mt-4" variant="outline" onclick={openAddNotifierDialog}>
										添加第一个通知器
									</Button>
								</div>
							{:else}
								<div class="space-y-3">
									{#each formData.notifiers as notifier, index (index)}
										<div class="flex items-center justify-between rounded-lg border p-4">
											<div class="flex-1">
												{#if notifier.type === 'telegram'}
													<div class="flex items-center gap-2">
														<Badge variant="secondary">Telegram</Badge>
														<span class="text-muted-foreground text-sm">
															Chat ID: {notifier.chat_id}
														</span>
													</div>
												{:else if notifier.type === 'webhook'}
													<div class="flex items-center gap-2">
														<Badge variant="secondary">Webhook</Badge>
														<span class="text-muted-foreground text-sm">
															{notifier.url}
														</span>
													</div>
												{:else if notifier.type === 'smtp'}
													<div class="flex flex-wrap items-center gap-2">
														<Badge variant="secondary">SMTP</Badge>
														<span class="text-muted-foreground text-sm break-all">
															{notifier.host}:{notifier.port} → {notifier.to}
														</span>
													</div>
												{/if}
											</div>
											<div class="flex gap-2">
												<Button
													size="sm"
													variant="outline"
													onclick={() => openEditNotifierDialog(notifier, index)}
												>
													编辑
												</Button>
												<Button
													size="sm"
													variant="secondary"
													onclick={() => testNotifier(notifier)}
												>
													测试
												</Button>
												<Button
													size="sm"
													variant="destructive"
													onclick={() => deleteNotifier(index)}
												>
													删除
												</Button>
											</div>
										</div>
									{/each}
								</div>
							{/if}
						</div>

						<Separator />

						<div class="flex items-center justify-between rounded-lg border p-4">
							<div class="space-y-1">
								<Label for="ignore-common-errors">忽略常见错误</Label>
								<p class="text-muted-foreground text-sm">
									接口偶发 -503 服务暂不可用或 -504
									服务调用超时，在通知器内容易刷屏，开启该选项以忽略此类错误通知
								</p>
							</div>
							<Switch id="ignore-common-errors" bind:checked={formData.ignore_common_errors} />
						</div>
					</div>
				</section>

				<!-- 下载与并发 -->
				<section id="section-download" class="bg-card scroll-mt-4 rounded-lg border p-5">
					<h2 class="text-base font-semibold">下载与并发</h2>
					<p class="text-muted-foreground mt-1 mb-4 text-sm">
						下载并发、请求频率限制与单文件分块下载
					</p>

					<div class="space-y-6">
						<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
							<div class="space-y-2">
								<Label for="video-concurrent">视频并发数</Label>
								<Input
									id="video-concurrent"
									type="number"
									min="1"
									max="20"
									bind:value={formData.concurrent_limit.video}
								/>
							</div>
							<div class="space-y-2">
								<Label for="page-concurrent">分页并发数</Label>
								<Input
									id="page-concurrent"
									type="number"
									min="1"
									max="20"
									bind:value={formData.concurrent_limit.page}
								/>
							</div>
						</div>

						<Separator />

						<div class="space-y-4">
							<div class="mb-4 flex items-center space-x-2">
								<Switch
									id="rate-limit-enable"
									checked={formData.concurrent_limit.rate_limit !== null &&
										formData.concurrent_limit.rate_limit !== undefined}
									onCheckedChange={(checked) => {
										if (checked) {
											formData!.concurrent_limit.rate_limit = { limit: 4, duration: 250 };
										} else {
											formData!.concurrent_limit.rate_limit = undefined;
										}
									}}
								/>
								<Label for="rate-limit-enable">启用请求频率限制</Label>
							</div>
							{#if formData.concurrent_limit.rate_limit}
								<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
									<div class="space-y-2">
										<Label for="rate-limit-duration">时间间隔（毫秒）</Label>
										<Input
											id="rate-limit-duration"
											type="number"
											min="100"
											bind:value={formData.concurrent_limit.rate_limit.duration}
										/>
										<p class="text-muted-foreground text-xs">请求限制的时间窗口（毫秒）</p>
									</div>
									<div class="space-y-2">
										<Label for="rate-limit-limit">限制请求数</Label>
										<Input
											id="rate-limit-limit"
											type="number"
											min="1"
											bind:value={formData.concurrent_limit.rate_limit.limit}
										/>
										<p class="text-muted-foreground text-xs">每个时间间隔内允许的最大请求数</p>
									</div>
								</div>
							{/if}
						</div>

						<Separator />

						<div class="space-y-4">
							<div class="mb-4 flex items-center space-x-2">
								<Switch
									id="download-enable"
									bind:checked={formData.concurrent_limit.download.enable}
								/>
								<Label for="rate-limit-duration">启用单文件分块下载</Label>
							</div>
							<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
								<div class="space-y-2">
									<Label for="download-concurrency">下载分块数</Label>
									<Input
										id="download-concurrency"
										type="number"
										min="1"
										max="16"
										disabled={!formData.concurrent_limit.download.enable}
										bind:value={formData.concurrent_limit.download.concurrency}
									/>
									<p class="text-muted-foreground text-xs">
										单文件将分为若干大小相同的块并行下载，所有分块下载完毕后合并
									</p>
								</div>
								<div class="space-y-2">
									<Label for="download-threshold">启用分块下载的文件大小阈值（字节）</Label>
									<Input
										id="download-threshold"
										type="number"
										min="1048576"
										disabled={!formData.concurrent_limit.download?.enable}
										bind:value={formData.concurrent_limit.download.threshold}
									/>
									<p class="text-muted-foreground text-xs">
										大于该阈值的文件才使用分块下载，文件过小时分块下载的拆分合并成本可能大于带来的增益
									</p>
								</div>
							</div>
						</div>
					</div>
				</section>
			</div>
		</div>

		<!-- 吸底保存条：未保存提示 + 保存入口，避免顶部/底部两个保存按钮 -->
		<div
			class="bg-background/85 sticky bottom-0 z-20 flex items-center justify-between gap-4 border-t px-4 py-3 backdrop-blur"
		>
			<div class="flex items-center gap-2 text-sm">
				{#if dirty}
					<span class="h-2 w-2 rounded-full bg-amber-500"></span>
					<span class="text-muted-foreground">有未保存的修改</span>
				{:else}
					<span class="h-2 w-2 rounded-full bg-emerald-500"></span>
					<span class="text-muted-foreground">已保存</span>
				{/if}
			</div>
			<Button onclick={saveConfig} disabled={saving} size="lg">
				{saving ? '保存中...' : '保存配置'}
			</Button>
		</div>
	{:else}
		<div class="flex items-center justify-center py-16">
			<div class="space-y-4 text-center">
				<p class="text-muted-foreground">请先进行前端认证以加载配置</p>
			</div>
		</div>
	{/if}
</div>

<Dialog.Root bind:open={showNotifierDialog}>
	<Dialog.Portal>
		<Dialog.Overlay class="bg-background/80 fixed inset-0 z-50 backdrop-blur-sm" />
		<Dialog.Content
			class="bg-background fixed top-[50%] left-[50%] z-50 grid max-h-[90vh] w-full max-w-lg translate-x-[-50%] translate-y-[-50%] gap-4 overflow-y-auto border p-6 shadow-lg duration-200 sm:rounded-lg"
		>
			<Dialog.Header>
				<Dialog.Title>
					{isEditing ? '编辑通知器' : '添加通知器'}
				</Dialog.Title>
				<Dialog.Description>配置通知器类型和参数</Dialog.Description>
			</Dialog.Header>

			{#if showNotifierDialog}
				<NotifierDialog
					notifier={editingNotifier}
					onSave={(notifier) => {
						if (isEditing && editingNotifierIndex !== null) {
							updateNotifier(editingNotifierIndex, notifier);
						} else {
							addNotifier(notifier);
						}
					}}
					onCancel={closeNotifierDialog}
				/>
			{/if}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>

<!-- QR 登录弹窗 -->
<Dialog.Root bind:open={showQrLoginDialog}>
	<Dialog.Portal>
		<Dialog.Overlay class="bg-background/80 fixed inset-0 z-50 backdrop-blur-sm" />
		<Dialog.Content
			class="bg-background fixed top-[50%] left-[50%] z-50 grid w-full max-w-md translate-x-[-50%] translate-y-[-50%] gap-4 border p-6 shadow-lg duration-200 sm:rounded-lg"
		>
			<Dialog.Header>
				<Dialog.Title>扫码登录</Dialog.Title>
				<Dialog.Description>使用哔哩哔哩 APP 扫描二维码登录</Dialog.Description>
			</Dialog.Header>

			<QrLogin bind:this={qrLoginComponent} onSuccess={handleQrLoginSuccess} />
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
