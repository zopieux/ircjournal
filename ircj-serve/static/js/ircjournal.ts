const RECONNECT_INTERVAL = 5_000

function scrollToCentered(elem) {
    elem.scrollIntoView({block: "center"})
}

function firstSelectionTarget(): string | null {
    const current = window.location.hash.substring(1).split(";")
    const [selection ] = current
    if (selection == null || selection === "") return null
    const range = selection.split("-")
    return range[0]
}

function setHash(selection: string, filter: string) {
    let [currSelection, currFilter] = window.location.hash.substring(1).split(";")
    if (selection != null) currSelection = selection
    if (filter != null) currFilter = filter
    window.location.hash = `#${currSelection || ""};${currFilter || ""}`
}

const localBool: (name: string) => [() => null | boolean, (val: boolean) => void] = (name) => [
    () => ({"null": null, "true": true, "false": false}["" + window.localStorage.getItem(name)]),
    (val) => window.localStorage.setItem(name, "" + val),
]

const localCheckbox = (id, cb: (checked: boolean) => any, exec: boolean) => {
    const [get, set] = localBool(id)
    const el = document.getElementById(id) as HTMLInputElement
    const curr = get()
    // If not saved yet, use HTML as default.
    if (curr === null) set(el.checked)
    else el.checked = curr
    el.addEventListener("change", () => {
        set(el.checked)
        if (!!cb) cb(el.checked)
    })
    if (exec && !!cb) cb(el.checked)
    return el
}

function app() {
    const messageTable = document.querySelector(".messages") as HTMLElement
    const bottomMark = document.getElementById("bottom") as HTMLElement
    const clearSelectionButton = document.getElementById("clear-selection") as HTMLButtonElement

    function findByIdOrTimestamp(idOrTs: string): [HTMLElement, string] {
        const byId = document.getElementById(idOrTs)
        if (byId) return [byId, idOrTs]
        const byTs = messageTable.querySelector(`.msg[data-timestamp="${idOrTs}"]`) as HTMLElement
        if (byTs) return [byTs, byTs.dataset.timestamp]
    }

    function selectSingleLine(idOrTs: string): HTMLElement {
        let elem
        const byId = document.getElementById(idOrTs)
        if (byId) {
            elem = byId
        } else {
            elem = messageTable.querySelector(`[data-timestamp="${idOrTs}"]`)
        }
        if (elem) elem.classList.add("highlight")
        return elem
    }

    function selectLines(rangeOfTs: string[]): HTMLElement {
        const [first, last] = rangeOfTs.map(e => parseInt(e, 10)).sort()
        let elem = null
        messageTable.querySelectorAll(".msg").forEach((e: HTMLElement) => {
            const ts = parseInt(e.dataset.timestamp, 10)
            if (first <= ts && ts <= last) {
                if (!elem) elem = e
                e.classList.add("highlight")
            }
        })
        return elem
    }

    function prepareSelection() {
        messageTable.classList.add("highlight")
        clearSelectionButton.disabled = false
        messageTable.querySelectorAll(".msg.highlight").forEach(e => e.classList.remove("highlight"))
    }

    function clearSelection() {
        messageTable.classList.remove("highlight")
        clearSelectionButton.disabled = true
        messageTable.querySelectorAll(".msg.highlight").forEach(e => e.classList.remove("highlight"))
    }

    function timestampClicked(e: Event, multiSelect: boolean) {
        e.preventDefault()
        const msg = (e.target as HTMLElement).parentElement.parentElement.parentElement

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

    function onHashChange() {
        const current = window.location.hash.substring(1).split(";")
        const [selection] = current
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
    }

    function instrument(message: HTMLElement) {
        message.addEventListener("click", e => timestampClicked(e, shiftPressed))
    }

    let shiftPressed = false
    document.addEventListener("keydown", (e) => {
        if (e.key === "Shift") shiftPressed = true
    })
    document.addEventListener("keyup", (e) => {
        if (e.key === "Shift") shiftPressed = false
    })
    window.addEventListener("hashchange", () => onHashChange())
    messageTable.querySelectorAll("a.tslink[href^='#']").forEach(instrument)

    onHashChange()

    const sel = firstSelectionTarget()
    if (!!sel) {
        const [highlighted] = findByIdOrTimestamp(sel)
        if (highlighted) setTimeout(() => scrollToCentered(highlighted), 250)
    }

    clearSelectionButton.addEventListener("click", (e) => {
        e.preventDefault()
        clearSelection()
    })

    const autoScroll = localCheckbox("auto-scroll", _ => maybeScroll(), false)

    const maybeScroll = () => {
        if (autoScroll.checked) bottomMark.scrollIntoView({block: "end"})
    }

    let liveStream = null
    const startLiveUpdates = () => {
        const url = (messageTable as HTMLElement).dataset.stream
        liveStream = new EventSource(url, {withCredentials: true})
        liveStream.onerror = () => {
            liveStream = null
            console.warn("disconnected from live updates:")
            setTimeout(startLiveUpdates, RECONNECT_INTERVAL)
        }
        liveStream.onmessage = (m) => {
            if (m.type === "message" && !!m.data) {
                messageTable.insertAdjacentHTML("beforeend", m.data)
                instrument(messageTable.lastElementChild as HTMLElement)
                maybeScroll()
                flashElem(messageTable.lastElementChild as HTMLElement, 30, 20)
            }
        }
        liveStream.onopen = () => console.debug("Now listening for updates on", url)
    }

    localCheckbox("show-join-part", checked => {
        messageTable.classList.toggle("hide-join-part", !checked)
    }, true)

    localCheckbox("live", checked => {
        autoScroll.disabled = !checked
        if (checked) {
            startLiveUpdates()
        } else if (!!liveStream) {
            liveStream.close()
            liveStream = null
        }
    }, true)
}

function flashElem(el: HTMLElement, duration: number, steps: number) {
    let t = steps * duration
    const flash = () => {
        el.style.backgroundColor = `rgba(218, 199, 129, ${t / (steps * duration)})`
        t -= steps
        if (t > 0) window.requestAnimationFrame(flash)
        else el.style.backgroundColor = "transparent"
    }
    window.requestAnimationFrame(flash)
}

(function ready(fn) {
    if (document.readyState !== "loading") {
        fn()
    } else {
        document.addEventListener("DOMContentLoaded", fn)
    }
})(app)
