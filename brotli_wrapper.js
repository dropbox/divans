(function() {
var brotliIframe = null;
var brotliWindow = null;

var brotliReady = false;
var brotliProcessing = null;
var brotliInputQueue = [];

window.onmessage = function(e) {
    brotliWindow = brotliIframe.contentWindow;
    if (e.source !== brotliWindow) {
        throw "Invalid source " + e.source;
    }
    msgtype = e.data[0];
    if (msgtype == "brotli-worker-ready") {
        if (brotliInputQueue.length > 0) {
            var queueEl = brotliInputQueue.shift();
            processBrotliNow(queueEl[0], queueEl[1]);
        } else {
            brotliReady = true;
        }
    }
    if (msgtype == "brotli-finished") {
        var outputLen = e.data[1];
        var intermediateRep = e.data[2];
        var originalInput = brotliProcessing;
        var finFunc = brotliFinished;
        brotliProcessing = null;
        brotliFinished = null;
        setTimeout(function() {
            finFunc(originalInput, outputLen, intermediateRep);
        }, 0);
    }
}

function processBrotliNow(arrayBuf, finishedFunc) {
    brotliProcessing = arrayBuf;
    brotliFinished = finishedFunc;
    brotliReady = false;
    setTimeout(function() {
        
        brotliWindow.postMessage(max_quality, "*");
        brotliWindow.postMessage(arrayBuf, "*", [arrayBuf]);
    }, 0);
}

function addToBrotliQueue(arrayBuf, finishedFunc) {
    if (brotliReady) {
        brotliReady = false;
        processBrotliNow(arrayBuf, finishedFunc);
    } else {
        brotliInputQueue.push([arrayBuf, finishedFunc]);
    }
}

function createBrotliIframe() {
    brotliIframe = document.createElement("iframe");
    brotliIframe.setAttribute("id", "brotli_iframe");
    brotliIframe.setAttribute("src", "brotli_iframe.html");
    brotliIframe.setAttribute("sandbox", "allow-scripts");
    brotliIframe.style.display = "none";
    document.body.appendChild(brotliIframe);
}

function init() {
    createBrotliIframe();
}
document.addEventListener("DOMContentLoaded", init);

function runBrotliDestroysInput(arrayBuf, finishedFunc) {
    if (!(arrayBuf instanceof ArrayBuffer)) {
        throw "Invalid input";
    }
    addToBrotliQueue(arrayBuf, finishedFunc);
}

window.Brotli = {
    init: init,
    runBrotliDestroysInput: runBrotliDestroysInput
};

})();
