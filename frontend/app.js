const formLibrary = {
  basic_profile: {
    title: "基础档案",
    fields: [
      { key: "name", label: "姓名", type: "text" },
      { key: "gender", label: "性别", type: "select" },
      { key: "age", label: "年龄", type: "number" },
      { key: "phone", label: "手机号", type: "tel" },
    ],
  },
};

const storageKeys = {
  backendUrl: "voice-form-fill.backendUrl",
  formId: "voice-form-fill.formId",
  audioUrl: "voice-form-fill.audioUrl",
  audioFormat: "voice-form-fill.audioFormat",
  sourceLang: "voice-form-fill.sourceLang",
  targetLang: "voice-form-fill.targetLang",
};

const elements = {
  backendUrl: document.querySelector("#backendUrl"),
  formId: document.querySelector("#formId"),
  audioUrl: document.querySelector("#audioUrl"),
  audioFormat: document.querySelector("#audioFormat"),
  sourceLang: document.querySelector("#sourceLang"),
  targetLang: document.querySelector("#targetLang"),
  transcript: document.querySelector("#transcript"),
  transcribeBtn: document.querySelector("#transcribeBtn"),
  clearTextBtn: document.querySelector("#clearTextBtn"),
  fillBtn: document.querySelector("#fillBtn"),
  resetFormBtn: document.querySelector("#resetFormBtn"),
  transcribeStatus: document.querySelector("#transcribeStatus"),
  transcribeHint: document.querySelector("#transcribeHint"),
  fillStatus: document.querySelector("#fillStatus"),
  missingFields: document.querySelector("#missingFields"),
  invalidFields: document.querySelector("#invalidFields"),
  warnings: document.querySelector("#warnings"),
  responsePreview: document.querySelector("#responsePreview"),
  form: document.querySelector("#profileForm"),
};

bootstrap();

function bootstrap() {
  hydrateSavedSettings();
  bindEvents();
  setTranscribeState("idle", "请输入一个后端可访问的语音文件 URL。");
}

function bindEvents() {
  elements.transcribeBtn.addEventListener("click", handleTranscribe);
  elements.clearTextBtn.addEventListener("click", clearTranscript);
  elements.fillBtn.addEventListener("click", handleAutoFill);
  elements.resetFormBtn.addEventListener("click", resetForm);

  [
    elements.backendUrl,
    elements.formId,
    elements.audioUrl,
    elements.audioFormat,
    elements.sourceLang,
    elements.targetLang,
  ].forEach((element) => {
    element.addEventListener("change", persistSettings);
  });
}

function hydrateSavedSettings() {
  const savedBackend = localStorage.getItem(storageKeys.backendUrl);
  const savedFormId = localStorage.getItem(storageKeys.formId);
  const savedAudioUrl = localStorage.getItem(storageKeys.audioUrl);
  const savedAudioFormat = localStorage.getItem(storageKeys.audioFormat);
  const savedSourceLang = localStorage.getItem(storageKeys.sourceLang);
  const savedTargetLang = localStorage.getItem(storageKeys.targetLang);

  if (savedBackend) {
    elements.backendUrl.value = savedBackend;
  }
  if (savedFormId && formLibrary[savedFormId]) {
    elements.formId.value = savedFormId;
  }
  if (savedAudioUrl) {
    elements.audioUrl.value = savedAudioUrl;
  }
  if (savedAudioFormat) {
    elements.audioFormat.value = savedAudioFormat;
  }
  if (savedSourceLang) {
    elements.sourceLang.value = savedSourceLang;
  }
  if (savedTargetLang) {
    elements.targetLang.value = savedTargetLang;
  }
}

function persistSettings() {
  localStorage.setItem(storageKeys.backendUrl, elements.backendUrl.value.trim());
  localStorage.setItem(storageKeys.formId, elements.formId.value);
  localStorage.setItem(storageKeys.audioUrl, elements.audioUrl.value.trim());
  localStorage.setItem(storageKeys.audioFormat, elements.audioFormat.value);
  localStorage.setItem(storageKeys.sourceLang, elements.sourceLang.value.trim());
  localStorage.setItem(storageKeys.targetLang, elements.targetLang.value.trim());
}

async function handleTranscribe() {
  const backendUrl = normalizeBackendUrl(elements.backendUrl.value);
  const audioUrl = elements.audioUrl.value.trim();
  const audioFormat = elements.audioFormat.value.trim();
  const sourceLang = elements.sourceLang.value.trim();
  const targetLang = elements.targetLang.value.trim() || "zh";

  if (!audioUrl) {
    setTranscribeState("error", "请先填写语音文件 URL。");
    return;
  }

  let parsedUrl;
  try {
    parsedUrl = new URL(audioUrl);
  } catch {
    setTranscribeState("error", "语音文件 URL 格式不合法。");
    return;
  }

  if (!["http:", "https:"].includes(parsedUrl.protocol)) {
    setTranscribeState("error", "语音文件 URL 必须以 http 或 https 开头。");
    return;
  }

  elements.transcribeBtn.disabled = true;
  setTranscribeState("live", "正在调用后端 /translate/media 转文字...");

  try {
    const response = await fetch(`${backendUrl}/translate/media`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
      },
      body: JSON.stringify({
        source_lang: sourceLang || undefined,
        target_lang: targetLang,
        audio: {
          data: audioUrl,
          format: audioFormat,
        },
      }),
    });

    const payload = await response.json();
    elements.responsePreview.textContent = JSON.stringify(payload, null, 2);

    if (!response.ok || !payload.ok) {
      const message = payload?.error?.message || `HTTP ${response.status}`;
      throw new Error(message);
    }

    const translatedText = (payload.translated_text || "").trim();
    if (!translatedText) {
      throw new Error("后端没有返回可用的转写文本");
    }

    elements.transcript.value = translatedText;
    setTranscribeState("done", "语音已成功转成文字，可以执行自动填充。");
  } catch (error) {
    setTranscribeState("error", `转文字失败：${error.message}`);
  } finally {
    elements.transcribeBtn.disabled = false;
  }
}

function clearTranscript() {
  elements.transcript.value = "";
  setTranscribeState("idle", "识别文本已清空，可以重新调用语音转文字。");
}

async function handleAutoFill() {
  const text = elements.transcript.value.trim();
  if (!text) {
    setFillState("error", "请先完成语音转文字，或手动输入文字。");
    return;
  }

  const backendUrl = normalizeBackendUrl(elements.backendUrl.value);
  const formId = elements.formId.value;

  setFillState("live", "正在调用后端做结构化抽取...");
  elements.fillBtn.disabled = true;

  try {
    const response = await fetch(`${backendUrl}/extract/form`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
      },
      body: JSON.stringify({
        form_id: formId,
        text,
      }),
    });

    const payload = await response.json();
    elements.responsePreview.textContent = JSON.stringify(payload, null, 2);

    if (!response.ok || !payload.ok) {
      const message = payload?.error?.message || `HTTP ${response.status}`;
      throw new Error(message);
    }

    applyFormData(payload.data || {});
    renderList(elements.missingFields, payload.missing_fields, "missing");
    renderList(
      elements.invalidFields,
      (payload.invalid_fields || []).map((item) => `${item.field}: ${item.message}`),
      "invalid"
    );
    renderList(elements.warnings, payload.warnings, "warning");
    setFillState("done", "表单已根据转写文本自动填充。");
  } catch (error) {
    renderList(elements.invalidFields, [error.message], "invalid");
    setFillState("error", `自动填充失败：${error.message}`);
  } finally {
    elements.fillBtn.disabled = false;
  }
}

function applyFormData(data) {
  const currentForm = formLibrary[elements.formId.value];
  currentForm.fields.forEach((field) => {
    const input = document.querySelector(`#field-${field.key}`);
    if (!input) {
      return;
    }
    const value = data[field.key];
    input.value = value === null || value === undefined ? "" : String(value);
  });
}

function resetForm() {
  elements.form.reset();
  renderList(elements.missingFields, [], "missing");
  renderList(elements.invalidFields, [], "invalid");
  renderList(elements.warnings, [], "warning");
  setFillState("idle", "表单已清空。");
}

function renderList(container, items, kind) {
  container.innerHTML = "";
  if (!items || items.length === 0) {
    container.classList.add("empty-state");
    container.textContent = "暂无";
    return;
  }

  container.classList.remove("empty-state");
  items.forEach((item) => {
    const chip = document.createElement("span");
    chip.className = `chip ${kind}`;
    chip.textContent = item;
    container.appendChild(chip);
  });
}

function normalizeBackendUrl(rawUrl) {
  const trimmed = rawUrl.trim();
  return trimmed.endsWith("/") ? trimmed.slice(0, -1) : trimmed;
}

function setTranscribeState(state, message) {
  elements.transcribeStatus.className = `badge ${state}`;
  elements.transcribeStatus.textContent = transcribeBadgeLabel(state);
  elements.transcribeHint.textContent = message;
}

function setFillState(state, message) {
  elements.fillStatus.className = `badge ${state}`;
  elements.fillStatus.textContent = fillBadgeLabel(state);
  elements.responsePreview.dataset.status = state;
  if (state !== "done") {
    elements.responsePreview.textContent = JSON.stringify(
      {
        ok: state !== "error",
        message,
      },
      null,
      2
    );
  }
}

function transcribeBadgeLabel(state) {
  switch (state) {
    case "live":
      return "转写中";
    case "done":
      return "已完成";
    case "error":
      return "失败";
    default:
      return "待开始";
  }
}

function fillBadgeLabel(state) {
  switch (state) {
    case "live":
      return "处理中";
    case "done":
      return "已填充";
    case "error":
      return "失败";
    default:
      return "待执行";
  }
}
