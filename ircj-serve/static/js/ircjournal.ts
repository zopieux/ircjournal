const kReconnectInterval = 5_000
const kFlashLiveUpdates = true

const kHighlightClass = "highlight"
const kHideClass = "hide"
const kHideJoinPartClass = "hide-join-part"

function scrollToCentered(elem: HTMLElement) {
    elem.scrollIntoView({block: "center"})
}

function parseHash(): [string, string] {
    const [selection, filter] = decodeURIComponent(window.location.hash.substring(1)).split(";")
    return [selection || "", filter || ""]
}

function firstSelectionTarget(): string | null {
    const [selection] = parseHash()
    if (selection === "") return null
    const range = selection.split("-")
    return range[0]
}

function setHash(selection: string | null, filter: string | null) {
    let [currSelection, currFilter] = parseHash()
    if (selection !== null) currSelection = selection
    if (filter !== null) currFilter = filter
    window.location.hash = `#${currSelection || ""};${currFilter || ""}`
}

function localBool(name: string): [() => null | boolean, (val: boolean) => void] {
    return [
        () => ({"null": null, "true": true, "false": false}["" + window.localStorage.getItem(name)]),
        (val) => window.localStorage.setItem(name, val ? "true" : "false"),
    ]
}

function localCheckbox(id: string, cb: (checked: boolean) => void, exec: boolean) {
    const [get, set] = localBool(id)
    const el = document.getElementById(id) as HTMLInputElement
    const curr = get()
    // If not saved yet, use HTML as default.
    if (curr === null) set(el.checked)
    else el.checked = curr
    el.addEventListener("change", () => {
        set(el.checked)
        if (cb) cb(el.checked)
    })
    if (exec && !!cb) cb(el.checked)
    return el
}

function flashElem(el: HTMLElement, duration: number, steps: number) {
    if (!kFlashLiveUpdates) return
    let t = steps * duration
    function flash() {
        el.style.backgroundColor = `rgba(218, 199, 129, ${t / (steps * duration)})`
        t -= steps
        if (t > 0) window.requestAnimationFrame(flash)
        else el.style.backgroundColor = "transparent"
    }
    window.requestAnimationFrame(flash)
}

function app() {
    const messageTable = document.querySelector(".messages") as HTMLElement
    const bottomMark = document.getElementById("bottom")
    const clearSelectionButton = document.getElementById("clear-selection") as HTMLButtonElement
    const filterInput = document.getElementById("filter") as HTMLInputElement

    let shiftPressed = false
    let filterInputDebounce = null

    function localLineFilter(filter: string) {
        if (filter.length) {
            const re = new RegExp(filter, "i")
            Array.from(messageTable.querySelectorAll(".msg"))
                .map(m => [m, re.test(m.textContent)] as [HTMLElement, boolean])
                .forEach(([m, matches]) => m.classList.toggle(kHideClass, !matches))
        } else {
            Array.from(messageTable.querySelectorAll(".msg.hide"))
                .forEach(m => m.classList.remove(kHideClass))
        }
    }

    function findByIdOrTimestamp(idOrTs: string): [HTMLElement, string] {
        const byId = document.getElementById(idOrTs)
        if (byId) return [byId, idOrTs]
        const byTs = messageTable.querySelector(`.msg[data-timestamp="${idOrTs}"]`) as HTMLElement
        if (byTs) return [byTs, (byTs.dataset as { timestamp: never }).timestamp]
    }

    function selectSingleLine(idOrTs: string): HTMLElement {
        let elem: HTMLElement
        const byId = document.getElementById(idOrTs)
        if (byId) {
            elem = byId
        } else {
            elem = messageTable.querySelector(`[data-timestamp="${idOrTs}"]`)
        }
        if (elem) elem.classList.add(kHighlightClass)
        return elem
    }

    function selectLines(rangeOfTs: string[]): HTMLElement {
        const [first, last] = rangeOfTs.map(e => parseInt(e, 10)).sort()
        let elem: HTMLElement = null
        messageTable.querySelectorAll(".msg").forEach((e: HTMLElement) => {
            const ts = parseInt(e.dataset.timestamp, 10)
            if (first <= ts && ts <= last) {
                if (!elem) elem = e
                e.classList.add(kHighlightClass)
            }
        })
        return elem
    }

    function prepareSelection() {
        messageTable.classList.add(kHighlightClass)
        clearSelectionButton.disabled = false
        messageTable.querySelectorAll(".msg.highlight").forEach(e => e.classList.remove(kHighlightClass))
    }

    function clearSelection() {
        messageTable.classList.remove(kHighlightClass)
        clearSelectionButton.disabled = true
        messageTable.querySelectorAll(".msg.highlight").forEach(e => e.classList.remove(kHighlightClass))
    }

    function timestampClicked(e: Event, multiSelect: boolean) {
        e.preventDefault()
        const msg = (e.target as HTMLElement).parentElement.parentElement

        if (multiSelect) {
            const toTs = msg.dataset.timestamp
            let fromTarget = firstSelectionTarget()
            const [fromElem] = findByIdOrTimestamp(fromTarget)
            if (fromElem) fromTarget = fromElem.dataset.timestamp
            setHash(`${fromTarget}-${toTs}`, null)
        } else {
            setHash(msg.id, null)
        }
    }

    function onHashChange(updateInput: boolean) {
        const [selection, filter] = parseHash()
        if (selection != null) {
            if (selection === "") {
                clearSelection()
            } else {
                prepareSelection()
                const range = selection.split("-")
                if (range.length === 1) {
                    selectSingleLine(range[0])
                } else {
                    selectLines(range)
                }
            }
        }
        if (updateInput) {
            filterInput.value = filter
        }
        localLineFilter(filter)
    }

    function instrumentForTsClick(message: HTMLElement) {
        message.addEventListener("click", e => timestampClicked(e, shiftPressed))
    }

    document.addEventListener("keydown", (e) => {
        if (e.key === "Shift") shiftPressed = true
    })
    document.addEventListener("keyup", (e) => {
        if (e.key === "Shift") shiftPressed = false
    })
    window.addEventListener("hashchange", () => onHashChange(false))

    const doFilter = (e: Event) => {
        e.preventDefault()
        e.stopPropagation()
        clearTimeout(filterInputDebounce)
        filterInputDebounce = setTimeout(() => setHash(null, filterInput.value), 300)
    }
    filterInput.addEventListener("input", e => doFilter(e))
    filterInput.addEventListener("search", e => doFilter(e))

    messageTable.querySelectorAll("a.tslink[href^='#']").forEach(instrumentForTsClick)

    onHashChange(true)

    const sel = firstSelectionTarget()
    if (sel) {
        const [highlighted] = findByIdOrTimestamp(sel)
        if (highlighted) setTimeout(() => scrollToCentered(highlighted), 250)
    }

    clearSelectionButton.addEventListener("click", (e) => {
        e.preventDefault()
        clearSelection()
    })

    const autoScroll = localCheckbox("auto-scroll", () => maybeScroll(), false)

    function maybeScroll() {
        if (autoScroll.checked) bottomMark.scrollIntoView({block: "end"})
    }

    let liveStream: EventSource = null
    function startLiveUpdates() {
        const url = (messageTable.dataset as { stream: string }).stream
        liveStream = new EventSource(url, {withCredentials: true})
        liveStream.onerror = () => {
            // Some browsers have their own reconnection loop. Without close(), they would
            // race with our own retry, with exponential invocation growth.
            liveStream.close()
            liveStream = null
            console.warn("disconnected from live updates")
            if (liveCheckbox.checked) {
                setTimeout(() => startLiveUpdates(), kReconnectInterval)
            }
        }
        liveStream.onmessage = (m) => {
            if (m.type === "message" && !!m.data) {
                messageTable.insertAdjacentHTML("beforeend", m.data)
                instrumentForTsClick(messageTable.lastElementChild as HTMLElement)
                maybeScroll()
                flashElem(messageTable.lastElementChild as HTMLElement, 30, 20)
            }
        }
        liveStream.onopen = () => console.debug("Now listening for updates on", url)
    }

    localCheckbox("show-join-part", checked => {
        messageTable.classList.toggle(kHideJoinPartClass, !checked)
    }, true)

    const liveCheckbox = localCheckbox("live", checked => {
        autoScroll.disabled = !checked
        if (checked) {
            startLiveUpdates()
        } else if (liveStream) {
            liveStream.close()
            liveStream = null
        }
    }, true)
}

(function ready(fn) {
    if (document.readyState !== "loading") {
        fn()
    } else {
        document.addEventListener("DOMContentLoaded", fn)
    }
})(app)
