<template>
  <n-modal
    v-model:show="visible"
    preset="card"
    :title="t('settingsTitle')"
    style="width: 640px"
    @after-enter="load"
  >
    <n-form label-placement="left" label-width="110">
      <n-form-item :label="t('settingsLanguage')">
        <n-select v-model:value="lang" style="width: 200px" :options="langOptions" />
      </n-form-item>
      <n-form-item :label="t('settingsTheme')">
        <n-radio-group v-model:value="theme">
          <n-radio-button value="auto">{{ t("themeAuto") }}</n-radio-button>
          <n-radio-button value="light">{{ t("themeLight") }}</n-radio-button>
          <n-radio-button value="dark">{{ t("themeDark") }}</n-radio-button>
        </n-radio-group>
      </n-form-item>
      <n-form-item :label="t('settingsProxy')">
        <n-input-group>
          <n-input
            v-model:value="proxy"
            :placeholder="t('proxyPlaceholder')"
            :disabled="testing"
            clearable
          />
          <n-button :loading="testing" @click="runTest">{{ t("proxyTest") }}</n-button>
        </n-input-group>
      </n-form-item>
      <n-form-item label=" " v-if="testResult">
        <n-text :type="testResult.ok ? 'success' : 'error'" style="font-size: 12px">
          {{ testResult.message }}
        </n-text>
      </n-form-item>
      <n-form-item :label="t('settingsIncrementalDays')">
        <n-input-number
          v-model:value="incrementalDays"
          :min="1"
          :max="365"
          style="width: 160px"
        />
      </n-form-item>
      <n-form-item label=" ">
        <n-text depth="3" style="font-size: 12px">{{ t("settingsIncrementalHint") }}</n-text>
      </n-form-item>
      <n-form-item :label="t('settingsDb')">
        <n-text code style="font-size: 11px; word-break: break-all">{{ dbPath }}</n-text>
      </n-form-item>
    </n-form>
    <n-alert type="info" :bordered="false" style="font-size: 12px">
      {{ t("checkDisclaimer") }}
    </n-alert>

    <n-collapse style="margin-top: 12px">
      <n-collapse-item :title="t('webdavSection')" name="webdav">
        <!-- WebDAV 独立隐私警示：与链接检测提示保持分区独立 -->
        <n-alert type="warning" :bordered="false" style="font-size: 12px; margin-bottom: 12px">
          {{ t("webdavPrivacy") }}
        </n-alert>
        <n-form label-placement="left" label-width="110" size="small">
          <n-form-item :label="t('webdavUrl')">
            <n-input v-model:value="webdav.url" placeholder="https://dav.example.com" />
          </n-form-item>
          <n-form-item :label="t('webdavUser')">
            <n-input v-model:value="webdav.user" />
          </n-form-item>
          <n-form-item :label="t('webdavPassword')">
            <n-input
              v-model:value="webdav.password"
              type="password"
              show-password-on="click"
              autocomplete="off"
            />
          </n-form-item>
          <n-form-item :label="t('webdavDir')">
            <n-input v-model:value="webdav.dir" placeholder="/" />
          </n-form-item>
          <n-form-item :label="t('webdavAllowHttp')">
            <n-switch v-model:value="webdav.allowHttp" />
          </n-form-item>
          <n-form-item label=" ">
            <n-text depth="3" style="font-size: 12px">{{ t("webdavHttpsHint") }}</n-text>
          </n-form-item>
          <n-form-item label=" ">
            <n-space>
              <n-button size="small" :loading="webdavTesting" @click="runWebdavTest">
                {{ t("webdavTest") }}
              </n-button>
              <n-button size="small" :loading="webdavSaving" @click="saveWebdavConfigForm">
                {{ t("webdavSave") }}
              </n-button>
              <n-button size="small" :loading="webdavUploading" @click="uploadBackup">
                {{ t("webdavUpload") }}
              </n-button>
              <n-button size="small" @click="loadRemoteFiles">{{ t("webdavLoadList") }}</n-button>
            </n-space>
          </n-form-item>
          <n-form-item label=" " v-if="webdavTestResult">
            <n-text :type="webdavTestResult.ok ? 'success' : 'error'" style="font-size: 12px">
              {{ webdavTestResult.message }}
            </n-text>
          </n-form-item>
          <n-form-item label=" " v-if="webdavNotice">
            <n-text :type="webdavNoticeError ? 'error' : 'success'" style="font-size: 12px">
              {{ webdavNotice }}
            </n-text>
          </n-form-item>
          <n-form-item label=" " v-if="webdavUploadResult">
            <n-text style="font-size: 12px; word-break: break-all">
              {{ t("webdavBackupPath") }}: {{ webdavUploadResult.backupPath }}
              <br />
              {{ t("webdavRemotePath") }}: {{ webdavUploadResult.remotePath }}
            </n-text>
          </n-form-item>
          <n-form-item :label="t('webdavList')">
            <n-select
              v-model:value="selectedRemote"
              :options="remoteOptions"
              :placeholder="t('webdavList')"
              clearable
            />
          </n-form-item>
          <n-form-item label=" " v-if="remoteLoaded && webdavFiles.length === 0">
            <n-text depth="3" style="font-size: 12px">{{ t("webdavEmpty") }}</n-text>
          </n-form-item>
          <n-form-item label=" ">
            <n-button
              type="primary"
              size="small"
              :disabled="!selectedRemote"
              @click="openImportConfirm"
            >
              {{ t("webdavImport") }}
            </n-button>
          </n-form-item>
          <n-form-item label=" ">
            <n-alert type="warning" :bordered="false" style="font-size: 12px">
              {{ t("webdavLocalOnly") }} {{ t("webdavNeverOverwrite") }}
            </n-alert>
          </n-form-item>
        </n-form>
      </n-collapse-item>
    </n-collapse>

    <template #footer>
      <n-space justify="end">
        <n-button @click="visible = false">{{ t("cancel") }}</n-button>
        <n-button type="primary" :loading="saving" @click="save">{{ t("proxySave") }}</n-button>
      </n-space>
    </template>
  </n-modal>

  <!-- 下载并导入：显式确认框 + 替换/追加选择（沿用现有导入语义） -->
  <n-modal
    v-model:show="importConfirmVisible"
    preset="card"
    :title="t('webdavImportTitle')"
    style="width: 420px"
  >
    <n-space vertical>
      <n-text style="font-size: 13px; word-break: break-all">{{ selectedRemote }}</n-text>
      <n-radio-group v-model:value="importMode">
        <n-radio value="replace">{{ t("webdavImportReplace") }}</n-radio>
        <n-radio value="append">{{ t("webdavImportAppend") }}</n-radio>
      </n-radio-group>
      <n-alert type="warning" :bordered="false" style="font-size: 12px">
        {{ t("webdavImportConfirm") }}
      </n-alert>
      <n-text v-if="importError" type="error" style="font-size: 12px">{{ importError }}</n-text>
    </n-space>
    <template #footer>
      <n-space justify="end">
        <n-button @click="importConfirmVisible = false">{{ t("cancel") }}</n-button>
        <n-button type="primary" :loading="webdavImporting" @click="confirmImport">
          {{ t("confirm") }}
        </n-button>
      </n-space>
    </template>
  </n-modal>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import {
  NAlert,
  NButton,
  NCollapse,
  NCollapseItem,
  NForm,
  NFormItem,
  NInput,
  NInputGroup,
  NInputNumber,
  NModal,
  NRadio,
  NRadioButton,
  NRadioGroup,
  NSelect,
  NSpace,
  NSwitch,
  NText,
} from "naive-ui";
import { useI18n, type Locale } from "../i18n";
import { useAppStore } from "../stores/app";
import {
  api,
  DEFAULT_WEBDAV_CONFIG,
  getIncrementalDays,
  getWebDavConfig,
  saveWebDavConfig,
  setIncrementalDays,
} from "../api";
import type {
  ProxyTestResult,
  WebDavConfig,
  WebDavFileDto,
  WebDavUploadDto,
} from "../types";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{ (e: "update:show", v: boolean): void }>();
const { t, setLocale } = useI18n();
const store = useAppStore();

const visible = computed({
  get: () => props.show,
  set: (v) => emit("update:show", v),
});

const lang = ref<Locale>("zh");
const theme = ref<"auto" | "light" | "dark">("auto");
const proxy = ref("");
const incrementalDays = ref(7);
const testing = ref(false);
const saving = ref(false);
const testResult = ref<ProxyTestResult | null>(null);
const dbPath = ref("");

// #14 WebDAV 分区状态
const webdav = ref<WebDavConfig>({ ...DEFAULT_WEBDAV_CONFIG });
const webdavTesting = ref(false);
const webdavSaving = ref(false);
const webdavUploading = ref(false);
const webdavImporting = ref(false);
const webdavTestResult = ref<ProxyTestResult | null>(null);
const webdavUploadResult = ref<WebDavUploadDto | null>(null);
const webdavNotice = ref("");
const webdavNoticeError = ref(false);
const webdavFiles = ref<WebDavFileDto[]>([]);
const remoteLoaded = ref(false);
const selectedRemote = ref<string | null>(null);
const importConfirmVisible = ref(false);
const importMode = ref<"replace" | "append">("replace");
const importError = ref("");

const langOptions = [
  { label: "简体中文", value: "zh" },
  { label: "English", value: "en" },
];

const remoteOptions = computed(() =>
  webdavFiles.value.map((f) => ({
    label: `${f.name} (${formatSize(f.size)})`,
    value: f.name,
  }))
);

function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function setNotice(text: string, isError: boolean): void {
  webdavNotice.value = text;
  webdavNoticeError.value = isError;
}

async function load() {
  lang.value = (localStorage.getItem("bo.locale") as Locale) ?? "zh";
  theme.value = store.themeMode;
  testResult.value = null;
  try {
    proxy.value = await api.getProxy();
  } catch {
    proxy.value = "";
  }
  incrementalDays.value = await getIncrementalDays();
  try {
    dbPath.value = await api.getDbPath();
  } catch {
    dbPath.value = "";
  }
  // WebDAV：每次打开重新读取本机配置，清空上一次的临时结果
  try {
    webdav.value = await getWebDavConfig();
  } catch {
    webdav.value = { ...DEFAULT_WEBDAV_CONFIG };
  }
  webdavTestResult.value = null;
  webdavUploadResult.value = null;
  webdavFiles.value = [];
  remoteLoaded.value = false;
  selectedRemote.value = null;
  importConfirmVisible.value = false;
  importError.value = "";
  setNotice("", false);
}

async function runTest() {
  testing.value = true;
  testResult.value = null;
  try {
    testResult.value = await api.testProxy(proxy.value);
  } catch (e) {
    testResult.value = { ok: false, latencyMs: null, message: String(e) };
  } finally {
    testing.value = false;
  }
}

async function save() {
  saving.value = true;
  try {
    setLocale(lang.value);
    store.setThemeMode(theme.value);
    await api.setProxy(proxy.value.trim());
    await setIncrementalDays(incrementalDays.value);
    await store.refresh();
    visible.value = false;
  } finally {
    saving.value = false;
  }
}

// ---------------- #14 WebDAV ----------------

/** 先测后存：用当前表单测试，不写 settings。 */
async function runWebdavTest(): Promise<void> {
  webdavTesting.value = true;
  webdavTestResult.value = null;
  try {
    webdavTestResult.value = await api.webdavTest({ ...webdav.value });
  } catch (e) {
    webdavTestResult.value = { ok: false, latencyMs: null, message: String(e) };
  } finally {
    webdavTesting.value = false;
  }
}

/** 保存 5 个 webdav_* 键（凭据明文仅存本机数据库）。 */
async function saveWebdavConfigForm(): Promise<void> {
  webdavSaving.value = true;
  try {
    await saveWebDavConfig(webdav.value);
    setNotice(t("webdavSaved"), false);
  } catch (e) {
    setNotice(String(e), true);
  } finally {
    webdavSaving.value = false;
  }
}

/** 上传备份：后端自动生成时间戳 HTML 并以 PUT 上传。 */
async function uploadBackup(): Promise<void> {
  webdavUploading.value = true;
  webdavUploadResult.value = null;
  setNotice("", false);
  try {
    const result = await api.webdavUpload();
    webdavUploadResult.value = result;
    setNotice(t("webdavUploadOk", { bytes: result.bytes }), false);
  } catch (e) {
    setNotice(String(e), true);
  } finally {
    webdavUploading.value = false;
  }
}

/** 拉取远端 HTML 备份列表供用户选择。 */
async function loadRemoteFiles(): Promise<void> {
  setNotice("", false);
  try {
    webdavFiles.value = await api.webdavList();
    remoteLoaded.value = true;
    selectedRemote.value = webdavFiles.value.length > 0 ? webdavFiles.value[0].name : null;
    if (webdavFiles.value.length === 0) setNotice(t("webdavEmpty"), false);
  } catch (e) {
    webdavFiles.value = [];
    remoteLoaded.value = false;
    selectedRemote.value = null;
    setNotice(String(e), true);
  }
}

function openImportConfirm(): void {
  importError.value = "";
  if (!selectedRemote.value) {
    importError.value = t("webdavNoSelection");
    return;
  }
  importConfirmVisible.value = true;
}

/** 确认后下载并导入；替换/追加语义与本地 HTML 导入一致。 */
async function confirmImport(): Promise<void> {
  const name = selectedRemote.value;
  if (!name) {
    importError.value = t("webdavNoSelection");
    return;
  }
  webdavImporting.value = true;
  importError.value = "";
  try {
    const summary = await api.webdavImport(name, importMode.value);
    importConfirmVisible.value = false;
    setNotice(
      t("webdavImportOk", { b: summary.bookmarks, f: summary.folders }),
      false
    );
    await store.refresh();
  } catch (e) {
    importError.value = String(e);
  } finally {
    webdavImporting.value = false;
  }
}
</script>
