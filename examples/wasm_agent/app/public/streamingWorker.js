// Phi-1.5 WASM Worker with AutoAgents integration
console.log('Worker: Starting to import WASM modules...');
import init, {PhiModel, PhiAgentWrapper} from "./pkg/wasm_agent.js";

console.log('Worker: WASM modules imported successfully');

let wasmInitialized = false;
let currentModel = null;
let currentAgent = null;

// Initialize WASM when worker starts
const initializeWASM = async () => {
    try {
        console.log('Worker initialization started');
        await init();
        wasmInitialized = true;
        console.log('WASM initialized in worker successfully');
        return true;
    } catch (error) {
        console.error('Failed to initialize WASM in worker:', error);
        return false;
    }
};

// Download a file with caching and progress tracking
const downloadFile = async (url, description) => {
    const cacheName = "phi-model-cache";

    try {
        // Try to get from cache first
        const cache = await caches.open(cacheName);
        const cachedResponse = await cache.match(url);

        if (cachedResponse) {
            console.log(`Loading ${description} from cache:`, url);
            self.postMessage({type: 'loading_progress', message: `Loading ${description} from cache...`});

            if (cachedResponse.ok) {
                const data = await cachedResponse.arrayBuffer();
                console.log(`Cached ${description} size:`, data.byteLength);
                return data;
            } else {
                console.warn('Cached response not ok, removing from cache:', url);
                await cache.delete(url);
            }
        }

        // Download if not in cache
        console.log(`Downloading ${description}:`, url);
        self.postMessage({type: 'loading_progress', message: `Downloading ${description}...`});

        const response = await fetch(url, {
            method: 'GET',
            mode: 'cors',
            cache: "no-cache",
            redirect: "follow",
            credentials: 'omit'
        });

        if (!response.ok) {
            throw new Error(`HTTP ${response.status}: ${response.statusText} for ${url}`);
        }

        const contentLength = response.headers.get('content-length');
        const total = parseInt(contentLength, 10);
        let loaded = 0;

        const reader = response.body.getReader();
        const chunks = [];

        while (true) {
            const {done, value} = await reader.read();

            if (done) break;

            chunks.push(value);
            loaded += value.length;

            if (total && loaded > 0) {
                const progress = Math.round((loaded / total) * 100);
                self.postMessage({
                    type: 'loading_progress',
                    message: `Downloading ${description}... ${progress}% (${(loaded / 1024 / 1024).toFixed(1)} MB)`
                });
            }
        }

        // Concatenate all chunks
        const totalLength = chunks.reduce((acc, chunk) => acc + chunk.length, 0);
        const result = new Uint8Array(totalLength);
        let offset = 0;

        for (const chunk of chunks) {
            result.set(chunk, offset);
            offset += chunk.length;
        }

        console.log(`Downloaded ${description}: ${result.length} bytes`);

        // Cache the successful response
        try {
            const responseForCache = new Response(result.buffer, {
                status: 200,
                statusText: 'OK',
                headers: new Headers({
                    'Content-Type': 'application/octet-stream',
                    'Content-Length': result.length.toString()
                })
            });
            await cache.put(url, responseForCache);
            console.log(`Cached ${description} successfully`);
        } catch (cacheError) {
            console.warn(`Failed to cache ${description}:`, cacheError);
        }

        return result.buffer;

    } catch (error) {
        console.error(`Failed to download ${description}:`, error);
        throw error;
    }
};

// Load Phi model
const loadModel = async (modelConfig) => {
    try {
        console.log('Loading Phi model in worker:', modelConfig);
        self.postMessage({type: 'loading_progress', message: 'Starting Phi model load...'});

        // Download Phi model files
        const weightsUrl = `${modelConfig.base_url}${modelConfig.model}`;
        const tokenizerUrl = `${modelConfig.base_url}${modelConfig.tokenizer}`;
        const configUrl = `${modelConfig.base_url}${modelConfig.config}`;

        const [weightsBuffer, tokenizerBuffer, configBuffer] = await Promise.all([
            downloadFile(weightsUrl, 'Phi model weights'),
            downloadFile(tokenizerUrl, 'Phi tokenizer'),
            downloadFile(configUrl, 'Phi config')
        ]);

        const weightsArray = new Uint8Array(weightsBuffer);
        const tokenizerArray = new Uint8Array(tokenizerBuffer);
        const configArray = new Uint8Array(configBuffer);

        console.log('Initializing Phi model with weights:', weightsArray.length, 'tokenizer:', tokenizerArray.length, 'config:', configArray.length);
        self.postMessage({type: 'loading_progress', message: 'Initializing Phi model...'});

        const isQuantized = modelConfig.quantized || false;
        currentModel = new PhiModel(weightsArray, tokenizerArray, configArray, isQuantized);

        console.log('Creating agent wrapper...');
        self.postMessage({type: 'loading_progress', message: 'Creating agent wrapper...'});

        // Create agent from the model
        try {
            currentAgent = await PhiAgentWrapper.from_phi_model(currentModel);
        } catch (error) {
            console.error('Failed to create agent wrapper:', error);
            throw error;
        }

        console.log('Phi agent loaded successfully');
        self.postMessage({type: 'model_loaded'});
        return true;

    } catch (error) {
        console.error('Failed to load model in worker:', error);
        self.postMessage({type: 'error', error: error.toString()});
        return false;
    }
};

// Generate response using the AutoAgents framework
const generateResponse = async (data) => {
    if (!currentAgent) {
        self.postMessage({type: 'error', error: 'No agent loaded'});
        return;
    }

    try {
        const {prompt, image} = data;
        console.log(`Starting generation for prompt: ${prompt}`);

        // Check if image was provided (Phi doesn't support images)
        if (image && image.data) {
            const response = "I'm a text generation model and can't analyze images. Please ask me text-based questions!";
            const words = response.split(' ');
            for (const word of words) {
                self.postMessage({type: 'token', token: word + ' '});
                await new Promise(resolve => setTimeout(resolve, 50));
            }
            self.postMessage({type: 'stream_complete'});
            return;
        }

        console.log('Getting real-time streaming response from Phi agent...');
        let startTime = performance.now();
        let tokenCount = 0;

        // Create a callback function to handle streaming tokens
        const tokenCallback = (token) => {
            console.log('JavaScript callback received token:', token);
            tokenCount++;
            const tokensSec = (tokenCount / (performance.now() - startTime)) * 1000;

            self.postMessage({
                type: 'token',
                token: token,
                tokensSec: tokensSec.toFixed(2),
                totalTime: performance.now() - startTime
            });
        };

        // Use the streaming method with real token-level streaming
        console.log('About to call currentAgent.get_response_stream...');
        try {
            console.log('Calling get_response_stream with prompt:', prompt.substring(0, 50));
            const result = await currentAgent.get_response_stream(prompt, tokenCallback);
            console.log('get_response_stream result:', result);
            console.log('Real-time streaming completed successfully, total tokens:', tokenCount);
        } catch (streamError) {
            console.log('Detailed streaming error:', streamError);
            console.log('Error type:', typeof streamError);
            console.log('Error message:', streamError?.message);
            console.log('Error toString:', streamError?.toString());
        }

        // Signal completion
        self.postMessage({type: 'stream_complete'});
        console.log('Generation fully completed, total tokens/words processed:', tokenCount);

    } catch (error) {
        console.error('Generation error in worker:', error);
        self.postMessage({type: 'error', error: error.toString()});
    }
};

// Cache management functions
const checkCacheStatus = async () => {
    try {
        const cacheName = "phi-model-cache";
        const cache = await caches.open(cacheName);
        const keys = await cache.keys();

        if (keys.length === 0) {
            self.postMessage({type: 'cache_status', status: "Cache is empty"});
        } else {
            const cacheInfo = await Promise.all(
                keys.map(async (request) => {
                    const response = await cache.match(request);
                    const size = response ? (await response.clone().arrayBuffer()).byteLength : 0;
                    return `${request.url.split('/').pop()}: ${(size / 1024 / 1024).toFixed(1)} MB`;
                })
            );
            self.postMessage({type: 'cache_status', status: `Cached files: ${cacheInfo.join(', ')}`});
        }
    } catch (error) {
        self.postMessage({type: 'cache_status', status: "Cache check failed"});
    }
};

const clearCache = async () => {
    try {
        const cacheName = "phi-model-cache";
        const cache = await caches.open(cacheName);
        const keys = await cache.keys();
        await Promise.all(keys.map(key => cache.delete(key)));
        self.postMessage({type: 'cache_status', status: "Cache cleared"});
        console.log("Cache cleared successfully");
    } catch (error) {
        self.postMessage({type: 'cache_status', status: "Failed to clear cache"});
        console.error("Failed to clear cache:", error);
    }
};

// Handle messages from main thread
self.onmessage = async (event) => {
    const {type, data} = event.data;
    console.log('Worker received message:', type);

    switch (type) {
        case 'init_wasm':
            const success = await initializeWASM();
            self.postMessage({type: 'wasm_initialized', success});
            break;

        case 'load_model':
            if (!wasmInitialized) {
                self.postMessage({type: 'error', error: 'WASM not initialized'});
                return;
            }
            await loadModel(data);
            break;

        case 'stream_tokens':
            await generateResponse(data);
            break;

        case 'check_cache':
            await checkCacheStatus();
            break;

        case 'clear_cache':
            await clearCache();
            break;

        default:
            console.warn('Unknown message type in worker:', type);
    }
};

console.log('Phi WASM worker initialized');