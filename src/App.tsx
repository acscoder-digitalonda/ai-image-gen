import { ChangeEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type StoredImageResponse = {
  id: string;
  name: string;
  mimeType: string;
  size: number;
  base64: string;
};

type LibraryImage = {
  id: string;
  name: string;
  src: string;
  size: number;
  mimeType: string;
  base64: string;
};

type GenerateImageResponse = {
  image: StoredImageResponse;
  revisedPrompt: string | null;
};

type PromptTemplate = {
  id: string;
  name: string;
  systemPrompt: string;
  userPrompt: string;
  dateCreated: number;
};

type GenerationLogEntry = {
  timestamp: number;
  prompt: string;
  systemPrompt: string | null;
  referenceImages: string[];
  outputImage: string;
};

const DEFAULT_PROMPT_TEMPLATE_NAME = "default";

const STORAGE_KEYS = {
  geminiApiKey: "settings.gemini.apiKey",
  geminiModel: "settings.gemini.model",
  settingsPanel: "ui.settingsPanel",
};

const DEFAULT_GEMINI_MODEL = "gemini-2.5-flash-image";
const GEMINI_MODEL_SUGGESTIONS = [
  "gemini-2.5-flash-image",
  "gemini-2.0-flash",
  "gemini-1.5-flash-latest",
  "gemini-1.5-pro-latest",
];

const formatFileSize = (bytes: number) => {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const size = bytes / Math.pow(1024, exponent);
  return `${size.toFixed(size >= 10 || exponent === 0 ? 0 : 1)} ${units[exponent]}`;
};

const formatTimestamp = (seconds: number) => {
  if (!Number.isFinite(seconds)) {
    return "Unknown time";
  }
  const date = new Date(seconds * 1000);
  if (Number.isNaN(date.getTime())) {
    return "Unknown time";
  }
  return date.toLocaleString();
};

const toLibraryImage = (item: StoredImageResponse): LibraryImage => ({
  id: item.id,
  name: item.name,
  mimeType: item.mimeType,
  size: item.size,
  src: `data:${item.mimeType};base64,${item.base64}`,
  base64: item.base64,
});

const readFileAsBase64 = (file: File): Promise<string> =>
  new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      const [, base64] = result.split(",", 2);
      if (!base64) {
        reject(new Error("Failed to parse file contents."));
        return;
      }
      resolve(base64);
    };
    reader.onerror = () => reject(reader.error ?? new Error("Failed to read file."));
    reader.readAsDataURL(file);
  });

function App() {
  const [systemPrompt, setSystemPrompt] = useState("");
  const [imagePrompt, setImagePrompt] = useState("");
  const [inputImages, setInputImages] = useState<LibraryImage[]>([]);
  const [outputImages, setOutputImages] = useState<LibraryImage[]>([]);
  const [previewImage, setPreviewImage] = useState<LibraryImage | null>(null);
  const [selectedInputIds, setSelectedInputIds] = useState<string[]>([]);
  const [selectedOutputIds, setSelectedOutputIds] = useState<string[]>([]);
  const [statusMessage, setStatusMessage] = useState("");
  const [geminiApiKey, setGeminiApiKey] = useState("");
  const [geminiModel, setGeminiModel] = useState(DEFAULT_GEMINI_MODEL);
  const [showApiKey, setShowApiKey] = useState(false);
  const [showSettings, setShowSettings] = useState(() => {
    if (typeof window === "undefined") return false;
    return window.localStorage.getItem(STORAGE_KEYS.settingsPanel) !== "closed";
  });
  const [promptsLoaded, setPromptsLoaded] = useState(false);
  const [promptTemplateId, setPromptTemplateId] = useState<string | null>(null);
  const [activeTemplateName, setActiveTemplateName] = useState(DEFAULT_PROMPT_TEMPLATE_NAME);
  const [promptTemplates, setPromptTemplates] = useState<PromptTemplate[]>([]);
  const [isTemplateManagerOpen, setIsTemplateManagerOpen] = useState(false);
  const [isCreateTemplateOpen, setIsCreateTemplateOpen] = useState(false);
  const [templateManagerError, setTemplateManagerError] = useState("");
  const [editingTemplateId, setEditingTemplateId] = useState<string | null>(null);
  const [editingTemplateName, setEditingTemplateName] = useState("");
  const [editingSystemPrompt, setEditingSystemPrompt] = useState("");
  const [editingUserPrompt, setEditingUserPrompt] = useState("");
  const [createTemplateName, setCreateTemplateName] = useState("");
  const [createSystemPrompt, setCreateSystemPrompt] = useState(systemPrompt);
  const [createUserPrompt, setCreateUserPrompt] = useState(imagePrompt);
  const [createTemplateError, setCreateTemplateError] = useState("");
  const [promptSuggestion, setPromptSuggestion] = useState<string | null>(null);
  const [isLogVisible, setIsLogVisible] = useState(false);
  const [generationLogs, setGenerationLogs] = useState<GenerationLogEntry[]>([]);
  const [isLogLoading, setIsLogLoading] = useState(false);
  const [logError, setLogError] = useState("");
  const [isGenerating, setIsGenerating] = useState(false);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const selectedInputImages = useMemo(() => {
    const imagesById = new Map(inputImages.map((image) => [image.id, image]));
    return selectedInputIds
      .map((id) => imagesById.get(id))
      .filter((image): image is LibraryImage => Boolean(image));
  }, [inputImages, selectedInputIds]);
  const effectiveModel = useMemo(() => {
    const trimmed = geminiModel.trim();
    return trimmed ? trimmed : DEFAULT_GEMINI_MODEL;
  }, [geminiModel]);

  const loadLibraries = useCallback(async () => {
    try {
      const [inputStored, outputStored] = await Promise.all([
        invoke<StoredImageResponse[]>("list_images"),
        invoke<StoredImageResponse[]>("list_output_images"),
      ]);
      const inputMapped = inputStored.map(toLibraryImage);
      const outputMapped = outputStored.map(toLibraryImage);
      setInputImages(inputMapped);
      setOutputImages(outputMapped);
      setSelectedInputIds((prev) =>
        prev.filter((id) => inputMapped.some((image) => image.id === id))
      );
      setSelectedOutputIds((prev) =>
        prev.filter((id) => outputMapped.some((image) => image.id === id))
      );
    } catch (error) {
      console.error(error);
      setStatusMessage("Unable to load image library.");
    }
  }, []);

  useEffect(() => {
    if (typeof window === "undefined") return;

    const storedApiKey = window.localStorage.getItem(STORAGE_KEYS.geminiApiKey);
    if (storedApiKey) {
      setGeminiApiKey(storedApiKey);
    }

    const storedModel = window.localStorage.getItem(STORAGE_KEYS.geminiModel);
    if (storedModel) {
      setGeminiModel(storedModel);
    }
  }, []);

  useEffect(() => {
    let isMounted = true;

    const bootstrapPrompts = async () => {
      try {
        const templates = await invoke<PromptTemplate[]>("list_prompt_templates");
        if (!isMounted) return;
        setPromptTemplates(templates);

        if (templates.length) {
          const preferred =
            templates.find((template) => template.name === DEFAULT_PROMPT_TEMPLATE_NAME) ??
            templates[0];
          setActiveTemplateName(preferred.name);
          setPromptTemplateId(preferred.id);
          setSystemPrompt(preferred.systemPrompt);
          setImagePrompt(preferred.userPrompt);
        } else {
          setActiveTemplateName(DEFAULT_PROMPT_TEMPLATE_NAME);
          setPromptTemplateId(null);
          setSystemPrompt("");
          setImagePrompt("");
        }
      } catch (error) {
        console.error(error);
        if (isMounted) {
          setStatusMessage("Unable to load prompt templates.");
          setPromptTemplateId(null);
          setActiveTemplateName(DEFAULT_PROMPT_TEMPLATE_NAME);
          setSystemPrompt("");
          setImagePrompt("");
        }
      } finally {
        if (isMounted) {
          setPromptsLoaded(true);
        }
      }
    };

    void bootstrapPrompts();

    return () => {
      isMounted = false;
    };
  }, []);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const trimmed = geminiModel.trim();
    if (!trimmed) {
      window.localStorage.removeItem(STORAGE_KEYS.geminiModel);
    } else {
      window.localStorage.setItem(STORAGE_KEYS.geminiModel, trimmed);
    }
  }, [geminiModel]);

  useEffect(() => {
    void loadLibraries();
  }, [loadLibraries]);

  useEffect(() => {
    if (typeof window === "undefined") return;
    window.localStorage.setItem(
      STORAGE_KEYS.settingsPanel,
      showSettings ? "open" : "closed"
    );
  }, [showSettings]);

  useEffect(() => {
    if (!previewImage) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setPreviewImage(null);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [previewImage]);

  const persistApiKey = useCallback((value: string) => {
    if (typeof window === "undefined") return;
    const trimmed = value.trim();
    if (!trimmed) {
      window.localStorage.removeItem(STORAGE_KEYS.geminiApiKey);
    } else {
      window.localStorage.setItem(STORAGE_KEYS.geminiApiKey, trimmed);
    }
  }, []);

  const refreshTemplates = useCallback(async () => {
    try {
      const templates = await invoke<PromptTemplate[]>("list_prompt_templates");
      setPromptTemplates(templates);
      return templates;
    } catch (error) {
      console.error(error);
      setStatusMessage("Unable to load prompt templates.");
      return null;
    }
  }, [setPromptTemplates, setStatusMessage]);

  const persistPrompts = useCallback(
    async (system: string, user: string) => {
      const templateName = activeTemplateName.trim();
      if (!templateName) {
        setStatusMessage("Name your prompt template before saving.");
        return;
      }

      try {
        const saved = await invoke<PromptTemplate>("save_prompts", {
          payload: {
            id: promptTemplateId,
            name: templateName,
            systemPrompt: system,
            userPrompt: user,
          },
        });
        setPromptTemplateId(saved.id);
        setActiveTemplateName(saved.name);
        setPromptTemplates((prev) => {
          if (!prev.length) {
            return [saved];
          }
          const index = prev.findIndex((item) => item.id === saved.id);
          if (index === -1) {
            return [...prev, saved];
          }
          const next = [...prev];
          next[index] = saved;
          return next;
        });
      } catch (error) {
        console.error(error);
        setStatusMessage("Failed to save prompts to disk.");
      }
    },
    [
      activeTemplateName,
      promptTemplateId,
      setActiveTemplateName,
      setPromptTemplateId,
      setPromptTemplates,
      setStatusMessage,
    ]
  );

  const handleOpenTemplateManager = useCallback(() => {
    setTemplateManagerError("");
    setEditingTemplateId(null);
    setEditingTemplateName("");
    setEditingSystemPrompt("");
    setEditingUserPrompt("");
    setIsTemplateManagerOpen(true);
    void refreshTemplates();
  }, [refreshTemplates, setIsTemplateManagerOpen, setTemplateManagerError]);

  const handleCloseTemplateManager = useCallback(() => {
    setIsTemplateManagerOpen(false);
    setTemplateManagerError("");
    setEditingTemplateId(null);
    setEditingTemplateName("");
    setEditingSystemPrompt("");
    setEditingUserPrompt("");
  }, [setIsTemplateManagerOpen, setTemplateManagerError]);

  const handleOpenCreateTemplate = useCallback(() => {
    setCreateTemplateError("");
    setCreateTemplateName("");
    setCreateSystemPrompt(systemPrompt);
    setCreateUserPrompt(imagePrompt);
    setIsCreateTemplateOpen(true);
  }, [imagePrompt, setCreateTemplateError, setCreateTemplateName, setCreateSystemPrompt, setCreateUserPrompt, systemPrompt]);

  const handleCloseCreateTemplate = useCallback(() => {
    setIsCreateTemplateOpen(false);
    setCreateTemplateError("");
    setCreateTemplateName("");
    setCreateSystemPrompt(systemPrompt);
    setCreateUserPrompt(imagePrompt);
  }, [imagePrompt, setCreateTemplateError, setCreateTemplateName, setCreateSystemPrompt, setCreateUserPrompt, systemPrompt]);

  const fetchGenerationLogs = useCallback(async () => {
    setIsLogLoading(true);
    try {
      const result = await invoke<GenerationLogEntry[]>("list_generation_logs");
      setGenerationLogs(result.slice().reverse());
      setLogError("");
    } catch (error) {
      console.error(error);
      setLogError("Unable to load generation logs.");
    } finally {
      setIsLogLoading(false);
    }
  }, []);

  const handleOpenLogs = useCallback(() => {
    setIsLogVisible(true);
    setLogError("");
    void fetchGenerationLogs();
  }, [fetchGenerationLogs]);

  const handleCloseLogs = useCallback(() => {
    setIsLogVisible(false);
    setLogError("");
  }, []);

  const handleSelectTemplate = useCallback(
    (template: PromptTemplate) => {
      setActiveTemplateName(template.name);
      setPromptTemplateId(template.id);
      setSystemPrompt(template.systemPrompt);
      setImagePrompt(template.userPrompt);
      setPromptSuggestion(null);
      setIsTemplateManagerOpen(false);
    },
    [
      setActiveTemplateName,
      setImagePrompt,
      setIsTemplateManagerOpen,
      setPromptTemplateId,
      setPromptSuggestion,
      setSystemPrompt,
    ]
  );

  const handleDeleteTemplate = useCallback(
    async (template: PromptTemplate) => {
      const confirmed =
        typeof window === "undefined" ||
        window.confirm(`Delete template "${template.name}"?`);

      if (!confirmed) {
        return;
      }

      try {
        await invoke("remove_prompts_by_id", { id: template.id });
        setPromptTemplates((prev) => {
          const next = prev.filter((item) => item.id !== template.id);
          if (template.id === promptTemplateId) {
            const fallback =
              next.find((item) => item.name === DEFAULT_PROMPT_TEMPLATE_NAME) ?? next[0];
            if (fallback) {
              setActiveTemplateName(fallback.name);
              setPromptTemplateId(fallback.id);
              setSystemPrompt(fallback.systemPrompt);
              setImagePrompt(fallback.userPrompt);
            } else {
              setActiveTemplateName(DEFAULT_PROMPT_TEMPLATE_NAME);
              setPromptTemplateId(null);
              setSystemPrompt("");
              setImagePrompt("");
            }
          }
          return next;
        });
        setStatusMessage(`Deleted template "${template.name}".`);
        void refreshTemplates();
      } catch (error) {
        console.error(error);
        setTemplateManagerError("Unable to delete template.");
      }
    },
    [
      promptTemplateId,
      setActiveTemplateName,
      setImagePrompt,
      setPromptTemplateId,
      setPromptTemplates,
      setStatusMessage,
      setSystemPrompt,
      setTemplateManagerError,
      refreshTemplates,
    ]
  );

  const handleCreateTemplateSubmit = useCallback(async () => {
    const trimmedName = createTemplateName.trim();
    const trimmedSystem = createSystemPrompt.trim();
    const trimmedUser = createUserPrompt.trim();

    if (!trimmedName) {
      setCreateTemplateError("Template needs a name.");
      return;
    }

    if (!trimmedSystem && !trimmedUser) {
      setCreateTemplateError("Add system or image prompt content.");
      return;
    }

    if (
      promptTemplates.some(
        (template) => template.name.toLowerCase() === trimmedName.toLowerCase()
      )
    ) {
      setCreateTemplateError("Template name already exists.");
      return;
    }

    try {
      const saved = await invoke<PromptTemplate>("save_prompts", {
        payload: {
          id: null,
          name: trimmedName,
          systemPrompt: trimmedSystem,
          userPrompt: trimmedUser,
        },
      });

      setPromptTemplates((prev) => [...prev, saved]);
      setPromptTemplateId(saved.id);
      setActiveTemplateName(saved.name);
      setSystemPrompt(saved.systemPrompt);
      setImagePrompt(saved.userPrompt);
      setPromptSuggestion(null);
      setStatusMessage(`Created template "${saved.name}".`);
      setIsCreateTemplateOpen(false);
      setCreateTemplateError("");
      void refreshTemplates();
    } catch (error) {
      console.error(error);
      setCreateTemplateError("Unable to create template.");
    }
  }, [
    createSystemPrompt,
    createTemplateName,
    createUserPrompt,
    promptTemplates,
    refreshTemplates,
    setActiveTemplateName,
    setCreateTemplateError,
    setImagePrompt,
    setPromptTemplateId,
    setPromptTemplates,
    setPromptSuggestion,
    setStatusMessage,
    setSystemPrompt,
  ]);

  const handleStartEditTemplate = useCallback(
    (template: PromptTemplate) => {
      setTemplateManagerError("");
      setEditingTemplateId(template.id);
      setEditingTemplateName(template.name);
      setEditingSystemPrompt(
        template.id === promptTemplateId ? systemPrompt : template.systemPrompt
      );
      setEditingUserPrompt(
        template.id === promptTemplateId ? imagePrompt : template.userPrompt
      );
    },
    [imagePrompt, promptTemplateId, setTemplateManagerError, systemPrompt]
  );

  const handleCancelEditTemplate = useCallback(() => {
    setEditingTemplateId(null);
    setEditingTemplateName("");
    setEditingSystemPrompt("");
    setEditingUserPrompt("");
    setTemplateManagerError("");
  }, [setTemplateManagerError]);

  const handleCommitEditTemplate = useCallback(
    async (template: PromptTemplate) => {
      const trimmedName = editingTemplateName.trim();
      const trimmedSystem = editingSystemPrompt.trim();
      const trimmedUser = editingUserPrompt.trim();

      if (!trimmedName) {
        setTemplateManagerError("Template name cannot be empty.");
        return;
      }

      if (!trimmedSystem && !trimmedUser) {
        setTemplateManagerError("Provide content for the template prompts.");
        return;
      }

      if (
        promptTemplates.some(
          (item) =>
            item.id !== template.id && item.name.toLowerCase() === trimmedName.toLowerCase()
        )
      ) {
        setTemplateManagerError("Another template already uses that name.");
        return;
      }

      try {
        const saved = await invoke<PromptTemplate>("save_prompts", {
          payload: {
            id: template.id,
            name: trimmedName,
            systemPrompt: trimmedSystem,
            userPrompt: trimmedUser,
          },
        });

        setPromptTemplates((prev) =>
          prev.map((item) => (item.id === saved.id ? saved : item))
        );
        if (template.id === promptTemplateId) {
          setActiveTemplateName(saved.name);
          setSystemPrompt(saved.systemPrompt);
          setImagePrompt(saved.userPrompt);
        }
        setEditingTemplateId(null);
        setEditingTemplateName("");
        setEditingSystemPrompt("");
        setEditingUserPrompt("");
        setTemplateManagerError("");
        setStatusMessage(`Updated template "${saved.name}".`);
        void refreshTemplates();
      } catch (error) {
        console.error(error);
        setTemplateManagerError("Unable to update template.");
      }
    },
    [
      editingTemplateName,
      editingSystemPrompt,
      editingUserPrompt,
      promptTemplateId,
      promptTemplates,
      refreshTemplates,
      setActiveTemplateName,
      setImagePrompt,
      setPromptTemplates,
      setStatusMessage,
      setSystemPrompt,
      setTemplateManagerError,
    ]
  );

  const handleUpload = useCallback(
    async (event: ChangeEvent<HTMLInputElement>) => {
      const { files } = event.currentTarget;
      if (!files?.length) return;

      const input = event.currentTarget;
      const fileArray = Array.from(files);

      try {
        const payloads = await Promise.all(
          fileArray.map(async (file) => ({
            fileName: file.name,
            mimeType: file.type || undefined,
            dataBase64: await readFileAsBase64(file),
          }))
        );

        await invoke<StoredImageResponse[]>("upload_images", { payloads });
        await loadLibraries();
        setStatusMessage(
          `Uploaded ${fileArray.length} image${fileArray.length === 1 ? "" : "s"}.`
        );
      } catch (error) {
        console.error(error);
        setStatusMessage("Failed to upload images.");
      } finally {
        input.value = "";
      }
    },
    [loadLibraries]
  );

  const removeAllOutputs = useCallback(async () => {
    if (!outputImages.length) return;

    const confirmed =
      typeof window === "undefined" ||
      window.confirm("Remove all images from the output folder?");

    if (!confirmed) {
      return;
    }

    const ids = outputImages.map((image) => image.id);

    try {
      await invoke("delete_output_images", { ids });
      setSelectedOutputIds([]);
      await loadLibraries();
      setStatusMessage(`Removed ${ids.length} output image${ids.length === 1 ? "" : "s"}.`);
    } catch (error) {
      console.error(error);
      setStatusMessage("Failed to remove output images.");
    }
  }, [outputImages, loadLibraries]);

  const removeInputImage = useCallback(
    async (image: LibraryImage) => {
      const { id, name } = image;
      try {
        await invoke("delete_images", { ids: [id] });
        setSelectedInputIds((prev) => prev.filter((item) => item !== id));
        await loadLibraries();
        setStatusMessage(`Removed reference image ${name}.`);
      } catch (error) {
        console.error(error);
        setStatusMessage("Failed to remove reference image.");
      }
    },
    [loadLibraries, setSelectedInputIds, setStatusMessage]
  );

  const removeOutputImage = useCallback(
    async (image: LibraryImage) => {
      const { id, name } = image;
      try {
        await invoke("delete_output_images", { ids: [id] });
        setSelectedOutputIds((prev) => prev.filter((item) => item !== id));
        setPreviewImage((prev) => (prev?.id === id ? null : prev));
        await loadLibraries();
        setStatusMessage(`Removed output image ${name}.`);
      } catch (error) {
        console.error(error);
        setStatusMessage("Failed to remove output image.");
      }
    },
    [loadLibraries, setSelectedOutputIds, setPreviewImage, setStatusMessage]
  );

  const toggleInputSelection = (id: string) => {
    setSelectedInputIds((prev) =>
      prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id]
    );
  };

  const toggleOutputSelection = (id: string) => {
    setSelectedOutputIds((prev) =>
      prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id]
    );
  };

  const clearInputSelection = () => setSelectedInputIds([]);

  const handleOpenOutputFolder = useCallback(async () => {
    try {
      const path = await invoke<string>("get_output_dir_path");
      await invoke("open_dir", { path });
    } catch (error) {
      console.error(error);
      setStatusMessage("Unable to open output folder.");
    }
  }, []);

  const handleGenerateImage = useCallback(async () => {
    const trimmedPrompt = imagePrompt.trim();
    const trimmedApiKey = geminiApiKey.trim();

    if (!trimmedPrompt) {
      setStatusMessage("Add an image prompt to start a generation request.");
      return;
    }

    if (!trimmedApiKey) {
      setStatusMessage("Add your Gemini API key in Settings to generate images.");
      return;
    }

    if (isGenerating) {
      return;
    }

    setIsGenerating(true);
    setStatusMessage("Generating image...");

    try {
      const trimmedSystemPrompt = systemPrompt.trim();
      const referenceImages = selectedInputImages.map((image, index) => ({
        mimeType: image.mimeType,
        dataBase64: image.base64.trim(),
        slot: `img_${index + 1}`,
        fileName: image.name,
      }));

      const payload: Record<string, unknown> = {
        apiKey: trimmedApiKey,
        model: effectiveModel,
        imagePrompt: trimmedPrompt,
      };

      if (trimmedSystemPrompt) {
        payload.systemPrompt = trimmedSystemPrompt;
      }

      if (referenceImages.length) {
        payload.referenceImages = referenceImages;
      }

      const response = await invoke<GenerateImageResponse>("generate_image", {
        payload,
      });

      let status = `Image generated and saved as ${response.image.name}.`;

      if (response.revisedPrompt) {
        const trimmedSuggestion = response.revisedPrompt.trim();
        if (trimmedSuggestion) {
          setPromptSuggestion(trimmedSuggestion);
          status = `${status} Suggested prompt available.`;
        } else {
          setPromptSuggestion(null);
        }
      } else {
        setPromptSuggestion(null);
      }

      setStatusMessage(status);
      void fetchGenerationLogs();

      if (promptsLoaded) {
        void persistPrompts(systemPrompt, imagePrompt);
      }

      await loadLibraries();
    } catch (error) {
      console.error(error);
      const message =
        typeof error === "string"
          ? error
          : error && typeof error === "object" && "message" in error
          ? String((error as { message?: unknown }).message ?? "Failed to generate image.")
          : "Failed to generate image.";
      setStatusMessage(message);
    } finally {
      setIsGenerating(false);
    }
  }, [
    effectiveModel,
    geminiApiKey,
    imagePrompt,
    isGenerating,
    loadLibraries,
    fetchGenerationLogs,
    setPromptSuggestion,
    persistPrompts,
    promptsLoaded,
    selectedInputImages,
    systemPrompt,
  ]);

  return (
    <>
      <main className="app-layout">
        <section className="prompt-panel">
        <header className="panel-header">
          <div>
            <h1>Image Generation</h1>
            <p className="panel-subtitle">
              Configure prompts on the left, manage reference images on the right.
            </p>
          </div>
          <button
            type="button"
            className="panel-toggle"
            onClick={() => setShowSettings((prev) => !prev)}
            aria-expanded={showSettings}
            aria-controls="gemini-settings"
          >
            <span aria-hidden="true">⚙</span>
            <span>{showSettings ? "Hide Settings" : "Show Settings"}</span>
          </button>
        </header>

        {showSettings && (
          <div className="settings-card" id="gemini-settings">
            <div className="settings-header">
              <div>
                <h2>Gemini Settings</h2>
                <p className="settings-footnote">Stored locally on this device only.</p>
              </div>
              <button
                type="button"
                className="ghost"
                onClick={handleOpenTemplateManager}
              >
                Manage Templates
              </button>
            </div>
            <div className="template-summary" role="status" aria-live="polite">
              <span>Active Template</span>
              <strong title={activeTemplateName}>{activeTemplateName}</strong>
            </div>
            <div className="settings-grid">
              <label className="settings-field">
                <span>API Key</span>
                <div className="settings-input-row">
                  <input
                    type={showApiKey ? "text" : "password"}
                    value={geminiApiKey}
                    onChange={(event) => {
                      const nextValue = event.target.value;
                      setGeminiApiKey(nextValue);
                      persistApiKey(nextValue);
                    }}
                    placeholder="Paste your Google Gemini API key"
                    autoComplete="off"
                    spellCheck={false}
                  />
                  <button
                    type="button"
                    className="ghost"
                    onClick={() => setShowApiKey((prev) => !prev)}
                  >
                    {showApiKey ? "Hide" : "Show"}
                  </button>
                </div>
              </label>

              <label className="settings-field">
                <span>Model Name</span>
                <input
                  list="gemini-model-suggestions"
                  value={geminiModel}
                  onChange={(event) => setGeminiModel(event.target.value)}
                  onBlur={() => {
                    if (!geminiModel.trim()) {
                      setGeminiModel(DEFAULT_GEMINI_MODEL);
                    }
                  }}
                  placeholder={DEFAULT_GEMINI_MODEL}
                  spellCheck={false}
                />
                <datalist id="gemini-model-suggestions">
                  {GEMINI_MODEL_SUGGESTIONS.map((modelName) => (
                    <option key={modelName} value={modelName} />
                  ))}
                </datalist>
              </label>
            </div>
          </div>
        )}

        {selectedInputImages.length > 0 && (
          <div className="selected-gallery">
            <p className="selected-summary">
              Selected {selectedInputImages.length} image
              {selectedInputImages.length === 1 ? "" : "s"}
            </p>
            <div className="selected-grid">
              {selectedInputImages.map((image, index) => (
                <div className="selected-item" key={image.id}>
                  <div className="selected-thumb">
                    <button
                      type="button"
                      className="selected-remove"
                      onClick={() => toggleInputSelection(image.id)}
                      aria-label={`Remove ${image.name} from selection`}
                    >
                      <span aria-hidden="true">×</span>
                    </button>
                    <img src={image.src} alt={image.name} />
                  </div>
                  <span className="selected-label">{`{img_${index + 1}}`}</span>
                </div>
              ))}
            </div>
          </div>
        )}

        <div className="prompt-template-toolbar">
          <span className="prompt-template-active" title={activeTemplateName}>
            Template: <strong>{activeTemplateName}</strong>
          </span>
          <button
            type="button"
            className="primary prompt-template-new"
            onClick={handleOpenCreateTemplate}
          >
            New Prompt
          </button>
          <button
            type="button"
            className="ghost"
            onClick={handleOpenTemplateManager}
          >
            Manage Prompts
          </button>
        </div>

        <div className="prompt-section">
          <label htmlFor="system-prompt">System Prompt</label>
          <textarea
            id="system-prompt"
            placeholder="Describe overall behaviour, persona, or guard rails for the model..."
            value={systemPrompt}
            onChange={(event) => {
              const nextValue = event.target.value;
              setSystemPrompt(nextValue);
              if (promptsLoaded) {
                void persistPrompts(nextValue, imagePrompt);
              }
              setPromptSuggestion(null);
            }}
          />
        </div>

        <div className="prompt-section">
          <label htmlFor="image-prompt">Image Prompt</label>
          <textarea
            id="image-prompt"
            placeholder="What should we create? Include subjects, styles, lighting, and mood."
            value={imagePrompt}
            onChange={(event) => {
              const nextValue = event.target.value;
              setImagePrompt(nextValue);
              if (promptsLoaded) {
                void persistPrompts(systemPrompt, nextValue);
              }
              setPromptSuggestion(null);
            }}
          />
          {promptSuggestion && (
            <div className="prompt-suggestion">
              <div className="prompt-suggestion-header">
                <span>Model suggestion</span>
                <button
                  type="button"
                  className="ghost"
                  onClick={() => setPromptSuggestion(null)}
                >
                  Dismiss
                </button>
              </div>
              <pre>{promptSuggestion}</pre>
              <div className="prompt-suggestion-actions">
                <button
                  type="button"
                  className="primary"
                  onClick={() => {
                    setImagePrompt(promptSuggestion);
                    setPromptSuggestion(null);
                    if (promptsLoaded) {
                      void persistPrompts(systemPrompt, promptSuggestion);
                    }
                  }}
                >
                  Use Suggested Prompt
                </button>
              </div>
            </div>
          )}
        </div>

        <div className="prompt-actions">
          <button
            type="button"
            className="primary"
            onClick={() => {
              void handleGenerateImage();
            }}
            disabled={isGenerating}
          >
            {isGenerating ? "Generating..." : "Generate Image"}
          </button>
          {statusMessage && <span className="status-message">{statusMessage}</span>}
        </div>
      </section>

      <section className="library-panel">
        <header className="panel-header">
          <div>
            <h2>Image Library</h2>
            <p className="panel-subtitle">
              Reference: {inputImages.length} item{inputImages.length === 1 ? "" : "s"} •{" "}
              {selectedInputIds.length} selected | Output: {outputImages.length} item
              {outputImages.length === 1 ? "" : "s"} • {selectedOutputIds.length} selected
            </p>
          </div>
        </header>

        <div className="library-section">
          <div className="library-section-header">
            <div>
              <h3>Reference Images</h3>
              <p>Managed from `src-tauri/input` and used as generation references.</p>
            </div>
            <div className="library-actions">
              <input
                ref={fileInputRef}
                type="file"
                accept="image/*"
                multiple
                onChange={(event) => {
                  void handleUpload(event);
                }}
                hidden
              />
              <button type="button" onClick={() => fileInputRef.current?.click()}>
                Upload Images
              </button>
              <button type="button" onClick={clearInputSelection} disabled={!selectedInputIds.length}>
                Clear Selection
              </button>
            </div>
          </div>

          <div className="image-grid">
            {inputImages.length ? (
              inputImages.map((image) => {
                const isSelected = selectedInputIds.includes(image.id);
                return (
                  <div
                    key={image.id}
                    className={`image-card${isSelected ? " selected" : ""}`}
                  >
                    <button
                      type="button"
                      className="image-preview"
                      onClick={() => {
                        toggleInputSelection(image.id);
                      }}
                    >
                      <img src={image.src} alt={image.name} />
                    </button>
                    <div
                      className="image-meta image-meta-inline image-meta-clickable"
                      role="button"
                      tabIndex={0}
                      onClick={() => {
                        toggleInputSelection(image.id);
                      }}
                      onKeyDown={(event) => {
                        if (event.key === "Enter" || event.key === " ") {
                          event.preventDefault();
                          toggleInputSelection(image.id);
                        }
                      }}
                    >
                      <span className="image-name" title={image.name}>
                        {image.name}
                      </span>
                      <button
                        type="button"
                        className="image-delete-button"
                        onClick={(event) => {
                          event.preventDefault();
                          event.stopPropagation();
                          void removeInputImage(image);
                        }}
                        aria-label={`Delete ${image.name}`}
                      >
                        🗑
                      </button>
                    </div>
                    {isSelected && <div className="selection-indicator" aria-hidden="true" />}
                  </div>
                );
              })
            ) : (
              <div className="empty-state">
                <p>No reference images yet. Upload files into the input folder to begin.</p>
              </div>
            )}
          </div>
        </div>

        <div className="library-section">
          <div className="library-section-header">
            <div className="library-title">
              <div className="library-title-row">
                <h3>Output Images</h3>
                <button
                  type="button"
                  className="icon-button"
                  onClick={() => {
                    void handleOpenOutputFolder();
                  }}
                  aria-label="Open output folder"
                >
                  📁
                </button>
              </div>
             
            </div>
            <div className="library-actions">
              <button
                type="button"
                className="danger"
                onClick={() => {
                  void removeAllOutputs();
                }}
                disabled={!outputImages.length}
              >
                Remove All Images
              </button>
            </div>
          </div>

          <div className="image-grid">
            {outputImages.length ? (
              outputImages.map((image) => {
                const isSelected = selectedOutputIds.includes(image.id);
                return (
                  <div
                    key={image.id}
                    className={`image-card${isSelected ? " selected" : ""}`}
                  >
                    <button type="button" className="image-preview" onClick={() => {
                      toggleOutputSelection(image.id);
                      setPreviewImage(image);
                    }}>
                      <img src={image.src} alt={image.name} />
                    </button>
                    <div className="image-meta image-meta-inline">
                      <span className="image-name" title={image.name}>
                        {image.name}
                      </span>
                      <button
                        type="button"
                        className="image-delete-button"
                        onClick={() => {
                          void removeOutputImage(image);
                        }}
                        aria-label={`Delete ${image.name}`}
                      >
                        🗑
                      </button>
                    </div>
                    {isSelected && <div className="selection-indicator" aria-hidden="true" />}
                  </div>
                );
              })
              
            ) : (
              <div className="empty-state">
                <p>No output images yet. Generate new assets to populate this section.</p>
              </div>
            )}
          </div>
        </div>
      </section>
    </main>

      <button
        type="button"
        className="log-button"
        onClick={handleOpenLogs}
        aria-label="View generation logs"
      >
        Log
      </button>

      {isTemplateManagerOpen && (
        <div
          className="template-manager-backdrop"
          role="dialog"
          aria-modal="true"
          aria-label="Manage prompt templates"
          onClick={handleCloseTemplateManager}
        >
          <div
            className="template-manager"
            onClick={(event) => {
              event.stopPropagation();
            }}
          >
            <header className="template-manager-header">
              <div>
                <h2>Prompt Templates</h2>
                <p>Switch between saved prompt variations for different tasks.</p>
              </div>
              <button
                type="button"
                className="template-manager-close"
                onClick={handleCloseTemplateManager}
                aria-label="Close template manager"
              >
                ×
              </button>
            </header>
            <div className="template-manager-body">
              {promptTemplates.length ? (
                <ul className="template-list">
                  {promptTemplates.map((template) => {
                    const isActive = template.id === promptTemplateId;
                    const isEditing = template.id === editingTemplateId;
                    return (
                      <li
                        key={template.id}
                        className={`template-list-item${isActive ? " active" : ""}`}
                      >
                        {isEditing ? (
                          <div className="template-edit">
                            <label>
                              <span>Template Name</span>
                              <input
                                type="text"
                                value={editingTemplateName}
                                onChange={(event) => {
                                  setEditingTemplateName(event.target.value);
                                  if (templateManagerError) {
                                    setTemplateManagerError("");
                                  }
                                }}
                                placeholder="Enter template name"
                                maxLength={60}
                              />
                            </label>
                            <label>
                              <span>System Prompt</span>
                              <textarea
                                value={editingSystemPrompt}
                                onChange={(event) => {
                                  setEditingSystemPrompt(event.target.value);
                                  if (templateManagerError) {
                                    setTemplateManagerError("");
                                  }
                                }}
                                placeholder="Describe how the model should behave"
                                rows={3}
                              />
                            </label>
                            <label>
                              <span>Image Prompt</span>
                              <textarea
                                value={editingUserPrompt}
                                onChange={(event) => {
                                  setEditingUserPrompt(event.target.value);
                                  if (templateManagerError) {
                                    setTemplateManagerError("");
                                  }
                                }}
                                placeholder="What should this template generate?"
                                rows={3}
                              />
                            </label>
                            <div className="template-edit-actions">
                              <button
                                type="button"
                                className="ghost"
                                onClick={handleCancelEditTemplate}
                              >
                                Cancel
                              </button>
                              <button
                                type="button"
                                className="primary"
                                onClick={() => {
                                  void handleCommitEditTemplate(template);
                                }}
                              >
                                Save
                              </button>
                            </div>
                          </div>
                        ) : (
                          <>
                            <button
                              type="button"
                              className="template-select"
                              onClick={() => handleSelectTemplate(template)}
                            >
                              <span className="template-name">{template.name}</span>
                              <span className="template-hint">
                                {isActive ? "Active" : "Load template"}
                              </span>
                            </button>
                            <div className="template-action-buttons">
                              <button
                                type="button"
                                className="ghost template-edit-button"
                                onClick={() => handleStartEditTemplate(template)}
                                aria-label={`Rename template ${template.name}`}
                              >
                                ✏️
                              </button>
                              <button
                                type="button"
                                className="image-delete-button template-delete"
                                onClick={() => {
                                  void handleDeleteTemplate(template);
                                }}
                                aria-label={`Delete template ${template.name}`}
                              >
                                🗑
                              </button>
                            </div>
                          </>
                        )}
                      </li>
                    );
                  })}
                </ul>
              ) : (
                <div className="template-empty">
                  <p>No prompt templates yet. Create one below to get started.</p>
                </div>
              )}
            </div>
            {templateManagerError && (
              <p className="template-error" role="alert">
                {templateManagerError}
              </p>
            )}
          </div>
        </div>
      )}

      {isLogVisible && (
        <div
          className="template-manager-backdrop"
          role="dialog"
          aria-modal="true"
          aria-label="Generation logs"
          onClick={handleCloseLogs}
        >
          <div
            className="log-modal"
            onClick={(event) => {
              event.stopPropagation();
            }}
          >
            <header className="log-modal-header">
              <div>
                <h2>Recent Generations</h2>
                <p>Last {generationLogs.length} prompts sent to the backend.</p>
              </div>
              <button
                type="button"
                className="template-manager-close"
                onClick={handleCloseLogs}
                aria-label="Close generation logs"
              >
                ×
              </button>
            </header>
            <div className="log-modal-body">
              {isLogLoading ? (
                <p>Loading logs…</p>
              ) : logError ? (
                <p className="template-error" role="alert">
                  {logError}
                </p>
              ) : generationLogs.length ? (
                <ul className="log-list">
                  {generationLogs.map((log) => (
                    <li key={`${log.timestamp}-${log.outputImage}`}>
                      <div className="log-entry-header">
                        <span className="log-entry-time">{formatTimestamp(log.timestamp)}</span>
                        <span className="log-entry-output">{log.outputImage}</span>
                      </div>
                      <div className="log-entry-section">
                        <strong>Prompt</strong>
                        <p>{log.prompt}</p>
                      </div>
                      {log.systemPrompt && (
                        <div className="log-entry-section">
                          <strong>System Prompt</strong>
                          <p>{log.systemPrompt}</p>
                        </div>
                      )}
                      {log.referenceImages.length > 0 && (
                        <div className="log-entry-section">
                          <strong>Reference Images</strong>
                          <ul className="log-entry-ref-list">
                            {log.referenceImages.map((path) => (
                              <li key={path}>{path}</li>
                            ))}
                          </ul>
                        </div>
                      )}
                    </li>
                  ))}
                </ul>
              ) : (
                <p>No generations logged yet.</p>
              )}
            </div>
          </div>
        </div>
      )}

      {isCreateTemplateOpen && (
        <div
          className="template-manager-backdrop"
          role="dialog"
          aria-modal="true"
          aria-label="Create prompt template"
          onClick={handleCloseCreateTemplate}
        >
          <div
            className="template-manager template-create"
            onClick={(event) => {
              event.stopPropagation();
            }}
          >
            <header className="template-manager-header">
              <div>
                <h2>New Prompt Template</h2>
                <p>Capture a reusable system and image prompt pair.</p>
              </div>
              <button
                type="button"
                className="template-manager-close"
                onClick={handleCloseCreateTemplate}
                aria-label="Close create prompt dialog"
              >
                ×
              </button>
            </header>
            <div className="template-form">
              <label>
                <span>Name</span>
                <input
                  type="text"
                  value={createTemplateName}
                  onChange={(event) => {
                    setCreateTemplateName(event.target.value);
                    if (createTemplateError) {
                      setCreateTemplateError("");
                    }
                  }}
                  placeholder="e.g. Moodboard Lighting"
                  maxLength={60}
                />
              </label>
              <label>
                <span>System Prompt</span>
                <textarea
                  value={createSystemPrompt}
                  onChange={(event) => {
                    setCreateSystemPrompt(event.target.value);
                    if (createTemplateError) {
                      setCreateTemplateError("");
                    }
                  }}
                  placeholder="Describe model behaviour or guidance"
                  rows={4}
                />
              </label>
              <label>
                <span>Image Prompt</span>
                <textarea
                  value={createUserPrompt}
                  onChange={(event) => {
                    setCreateUserPrompt(event.target.value);
                    if (createTemplateError) {
                      setCreateTemplateError("");
                    }
                  }}
                  placeholder="What should this template generate?"
                  rows={4}
                />
              </label>
              {createTemplateError && (
                <p className="template-error" role="alert">
                  {createTemplateError}
                </p>
              )}
            </div>
            <div className="template-modal-actions">
              <button type="button" className="ghost" onClick={handleCloseCreateTemplate}>
                Cancel
              </button>
              <button
                type="button"
                className="primary"
                onClick={() => {
                  void handleCreateTemplateSubmit();
                }}
              >
                Save Template
              </button>
            </div>
          </div>
        </div>
      )}

      {previewImage && (
        <div
          className="preview-backdrop"
          role="dialog"
          aria-modal="true"
          aria-label="Image preview"
          onClick={() => setPreviewImage(null)}
        >
          <div
            className="preview-dialog"
            onClick={(event) => {
              event.stopPropagation();
            }}
          >
            <button
              type="button"
              className="preview-close"
              onClick={() => setPreviewImage(null)}
              aria-label="Close preview"
            >
              ×
            </button>
            <div className="preview-image-wrapper">
              <img src={previewImage.src} alt={previewImage.name} />
            </div>
            <div className="preview-meta">
              <span className="preview-name" title={previewImage.name}>
                {previewImage.name}
              </span>
              <span className="preview-size">
                {previewImage.mimeType} • {formatFileSize(previewImage.size)}
              </span>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

export default App;
