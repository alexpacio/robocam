//! Small, local UI catalog. User-entered values and executable paths must use
//! the raw UTF-16 conversion instead of this display-text boundary.

use std::{
    borrow::Cow,
    sync::atomic::{AtomicBool, Ordering},
};

static ITALIAN: AtomicBool = AtomicBool::new(false);

pub fn set_language(language: &str) {
    let language = language.trim();
    let code = language.split(['-', '_']).next().unwrap_or(language);
    ITALIAN.store(
        code.eq_ignore_ascii_case("it") || language.eq_ignore_ascii_case("Italiano"),
        Ordering::Relaxed,
    );
}

pub fn translate(text: &str) -> Cow<'_, str> {
    if !ITALIAN.load(Ordering::Relaxed) {
        return Cow::Borrowed(text);
    }
    italian(text)
}

fn italian(text: &str) -> Cow<'_, str> {
    static_italian(text)
        .map(Cow::Borrowed)
        .unwrap_or_else(|| formatted_italian(text))
}

fn static_italian(text: &str) -> Option<&'static str> {
    let translated = match text {
        "&File" => "&File",
        "&Tools" => "&Strumenti",
        "Enable / pause\tF11" => "Attiva / pausa\tF11",
        "Enable / pause" => "Attiva / pausa",
        "Pause / enable" => "Pausa / attiva",
        "Show / hide click bar" => "Mostra / nascondi barra dei clic",
        "On-screen keyboard" => "Tastiera su schermo",
        "Exit" => "Esci",
        "Configuration…" => "Configurazione…",
        "Setup and calibration…" => "Configurazione guidata e calibrazione…",
        "Help and about…" => "Guida e informazioni…",
        "Configuration" => "Configurazione",
        "Keyboard" => "Tastiera",
        "Setup" => "Configurazione guidata",
        "Click bar" => "Barra dei clic",
        "Help" => "Guida",
        "Check for updates…" => "Cerca aggiornamenti…",
        "Robo is a built-in profile and cannot be deleted." => {
            "Robo è un profilo integrato e non può essere eliminato."
        }
        "Face-loss timeout (100–59000 ms)" => "Attesa perdita del viso (100–59000 ms)",
        "Stop pointer when face is not detected" => "Ferma il puntatore se il viso non è rilevato",
        "Open GitHub Releases" => "Apri le versioni su GitHub",
        "GitHub Releases: alexpacio/robocam" => "Versioni su GitHub: alexpacio/robocam",
        "Could not open your browser. Visit https://github.com/alexpacio/robocam/releases" => {
            "Impossibile aprire il browser. Visita https://github.com/alexpacio/robocam/releases"
        }
        "Check for updates at startup" => "Cerca aggiornamenti all'avvio",
        "No robocam release has been published yet." => {
            "Non è ancora stata pubblicata una versione di robocam."
        }
        "You are running the latest published version." => {
            "Stai usando l'ultima versione pubblicata."
        }
        "Camera preview" => "Anteprima della videocamera",
        "Camera preview\nWaiting for camera…" => {
            "Anteprima della videocamera\nIn attesa della videocamera…"
        }
        "Looking for your face" => "Ricerca del viso",
        "Camera unavailable\nChoose a camera in Configuration." => {
            "Videocamera non disponibile\nScegli una videocamera in Configurazione."
        }
        "Camera unavailable" => "Videocamera non disponibile",
        "Starting camera…" => "Avvio della videocamera…",
        "Opening camera…" => "Apertura della videocamera…",
        "Camera stopped. Open Configuration to restart it." => {
            "Videocamera arrestata. Apri Configurazione per riavviarla."
        }
        "No face — center your face in the camera" => {
            "Viso non rilevato: centralo nell'anteprima"
        }
        "Physical mouse in use — head control paused" => {
            "Mouse in uso: controllo con la testa in pausa"
        }
        "Tracking" => "Controllo attivo",
        "Paused · F11 to enable" => "In pausa · F11 per attivare",
        "Paused — use the enable button" => "In pausa — usa il pulsante di attivazione",
        "Clicks paused — dwell on Play to resume" => {
            "Click in pausa — soffermati su Play per riprendere"
        }
        "Automatic (ask when multiple)" => "Automatica (chiedi se più telecamere)",
        "Unavailable" => "Non disponibile",
        "Refresh cameras" => "Aggiorna telecamere",
        "Forget saved camera" => "Dimentica telecamera",
        "Multiple cameras detected. Choose a camera in Configuration and save the selection." => {
            "Sono presenti più telecamere. Scegline una in Configurazione e salva la scelta."
        }
        "No cameras detected. Connect a camera and refresh the camera list in Configuration." => {
            "Nessuna telecamera rilevata. Collegane una e aggiorna l'elenco in Configurazione."
        }
        "SketchUp: edit group/component" => "SketchUp: modifica gruppo/componente",
        "Use the SketchUp button over a group or component in the SketchUp model view." => {
            "Usa il pulsante SketchUp su un gruppo o componente nell'area del modello di SketchUp."
        }
        "Head tracking ready" => "Tracciamento della testa pronto",
        "Waiting for reliable head tracking…" => "In attesa del tracciamento della testa…",
        "Wait until head tracking is ready in the preview before starting." => {
            "Attendi che il tracciamento della testa sia pronto nell'anteprima prima di iniziare."
        }
        "Ready\n\nYour pointer speeds have been adjusted.\n\nUse the enable button to start or pause head control. Move your head to move the pointer; hold still to click.\n\nYou can tune speed, smoothing and dwell time in Configuration at any time." => {
            "Pronto\n\nLa velocità del puntatore è stata regolata.\n\nUsa il pulsante di attivazione per avviare o sospendere il controllo. Muovi la testa per spostare il puntatore; fermati per fare clic.\n\nPuoi regolare velocità, levigatezza e tempo di sosta in Configurazione."
        }
        "Show main window" => "Mostra finestra principale",
        "Open robocam" => "Apri robocam",
        "robocam click bar" => "robocam — Barra dei clic",
        "robocam dwell" => "robocam — Sosta",
        "robocam — Configuration" => "robocam — Configurazione",
        "robocam — Setup" => "robocam — Configurazione guidata",
        "robocam — F11 enable / pause" => "robocam — F11 attiva / pausa",
        "No click" => "No clic",
        "Left click" => "Clic sinistro",
        "Middle click" => "Clic centrale",
        "Right click" => "Clic destro",
        "Double click" => "Doppio clic",
        "Triple click" => "Triplo clic",
        "Drag" => "Trascina",
        "Scroll" => "Scorri",
        "Save as…" => "Salva come…",
        "Save" => "Salva",
        "Delete" => "Elimina",
        "Cancel" => "Annulla",
        "Pointer motion" => "Movimento del puntatore",
        "Clicking" => "Clic",
        "Camera" => "Videocamera",
        "General" => "Generale",
        "Horizontal speed (0–30)" => "Velocità orizzontale (0–30)",
        "Vertical speed (0–30)" => "Velocità verticale (0–30)",
        "Acceleration (0–5)" => "Accelerazione (0–5)",
        "Smoothing (0–8)" => "Fluidità (0–8)",
        "Easy stop (0–10)" => "Arresto facilitato (0–10)",
        "Arrow hotkeys adjust pointer speed" => {
            "I tasti freccia regolano la velocità del puntatore"
        }
        "Reverse horizontal direction" => "Inverti direzione orizzontale",
        "Reverse vertical direction" => "Inverti direzione verticale",
        "Restrict workspace" => "Limita area di lavoro",
        "Left margin (%)" => "Margine sinistro (%)",
        "Top margin (%)" => "Margine superiore (%)",
        "Right margin (%)" => "Margine destro (%)",
        "Bottom margin (%)" => "Margine inferiore (%)",
        "Wrap pointer at workspace edges" => "Porta il puntatore al bordo opposto",
        "Click method" => "Metodo di clic",
        "Mouse press duration (0-1000 ms)" => "Durata pressione mouse (0-1000 ms)",
        "Dwell" => "Sosta",
        "Gesture" => "Gesto",
        "Disabled" => "Disattivato",
        "Dwell time (200–5000 ms)" => "Durata della sosta (200–5000 ms)",
        "Movement tolerance (1–8 px)" => "Tolleranza di movimento (1–8 px)",
        "Delay between button events (0–50 ms)" => "Intervallo tra eventi del pulsante (0–50 ms)",
        "Allow consecutive clicks" => "Consenti clic consecutivi",
        "Fast gesture (fixed second dwell)" => "Gesto rapido (seconda sosta fissa)",
        "Play a sound on click" => "Riproduci un suono al clic",
        "Black cursor with gray outline during click" => {
            "Cursore nero con bordo grigio durante il clic"
        }
        "Show dwell progress by the pointer" => {
            "Mostra avanzamento della sosta vicino al puntatore"
        }
        "Pause after physical mouse (ms)" => "Pausa dopo uso del mouse (ms)",
        "Gesture: move left" => "Gesto: verso sinistra",
        "Gesture: move right" => "Gesto: verso destra",
        "Gesture: move up" => "Gesto: verso l'alto",
        "Gesture: move down" => "Gesto: verso il basso",
        "Gesture: dwell, move in a direction, dwell again." => {
            "Gesto: sosta, muoviti in una direzione, sosta di nuovo."
        }
        "Camera index (0 = default)" => "Indice videocamera (0 = predefinita)",
        "Locate face automatically" => "Rileva il viso automaticamente",
        "Face-loss timeout (100–3000 ms)" => "Attesa se il viso scompare (100–3000 ms)",
        "Face detection interval (50–2000 ms)" => "Intervallo rilevamento viso (50–2000 ms)",
        "For manual tracking, disable automatic face location," => {
            "Per l'inseguimento manuale, disattiva il rilevamento del viso,"
        }
        "save, then drag a rectangle over your face in the preview." => {
            "salva e traccia un rettangolo sul viso nell'anteprima."
        }
        "Use good front lighting and aim for at least 25 fps." => {
            "Illumina il viso frontalmente e punta ad almeno 25 fps."
        }
        "If the camera is unavailable, close other camera apps." => {
            "Se la videocamera non è disponibile, chiudi le altre app che la usano."
        }
        "Camera driver settings…" => "Impostazioni della videocamera…",
        "Reconnect camera" => "Ricollega videocamera",
        "Selection behavior" => "Modalità di selezione",
        "Normal (dwell to select)" => "Normale (sosta per selezionare)",
        "Fast (hover to select)" => "Rapida (passaggio per selezionare)",
        "Button style" => "Stile dei pulsanti",
        "48 px icons" => "Icone da 48 px",
        "Compact text" => "Testo compatto",
        "Position" => "Posizione",
        "Top" => "In alto",
        "Bottom" => "In basso",
        "Left" => "A sinistra",
        "Right" => "A destra",
        "Floating" => "Mobile",
        "Automatically hide bar until pointer nears edge" => {
            "Nascondi la barra finché il puntatore non si avvicina al bordo"
        }
        "Show click bar" => "Mostra barra dei clic",
        "The click bar stays visible until robocam exits." => {
            "La barra dei clic resta visibile fino alla chiusura di robocam."
        }
        "High-contrast bar (black background)" => "Barra ad alto contrasto (sfondo nero)",
        "Normal: select once for the next click; again to lock." => {
            "Normale: seleziona per il prossimo clic; riseleziona per fissare."
        }
        "Fast: hover for the next click; click to lock." => {
            "Rapida: passa per il prossimo clic; fai clic per fissare."
        }
        "Drag works with left, middle, or right single clicks." => {
            "Trascina funziona con clic singoli sinistri, centrali o destri."
        }
        "A second dwell releases the held button or ends scrolling." => {
            "Una seconda sosta rilascia il pulsante o termina lo scorrimento."
        }
        "On-screen keyboard executable" => "Programma della tastiera su schermo",
        "Enable / pause hotkey" => "Tasto per attivare / mettere in pausa",
        "Center pointer hotkey" => "Tasto per centrare il puntatore",
        "Workspace toggle hotkey" => "Tasto per limitare l'area di lavoro",
        "Enable head control at startup" => "Attiva il controllo con la testa all'avvio",
        "Start minimized in the notification area" => "Avvia ridotto nell'area di notifica",
        "Start robocam when I sign in to Windows" => "Avvia robocam all'accesso a Windows",
        "Closing the main window keeps robocam in the tray." => {
            "Chiudendo la finestra, robocam resta nell'area di notifica."
        }
        "Use File > Exit or the tray menu to stop it completely." => {
            "Per chiuderlo, usa File > Esci o il menu nell'area di notifica."
        }
        "Settings and profiles: %APPDATA%\\robocam\\settings.json" => {
            "Impostazioni e profili: %APPDATA%\\robocam\\settings.json"
        }
        "Language" => "Lingua",
        "Language (restart required)" => "Lingua (richiede riavvio)",
        "Language (restart to apply)" => "Lingua (riavvia per applicare)",
        "Language changes take effect after restarting robocam." => {
            "La lingua verrà applicata al riavvio di robocam."
        }
        "Restart robocam to apply the language change." => {
            "Riavvia robocam per applicare la nuova lingua."
        }
        "English" => "English",
        "Italiano" => "Italiano",
        "Next" => "Avanti",
        "Finish" => "Fine",
        "Retry" => "Riprova",
        "Welcome to robocam\n\nPosition your camera in front of you with good, even lighting. Your face should occupy roughly one third of the preview width.\n\nThis setup measures your comfortable head movement to adjust pointer speed. Head control is paused during calibration." => {
            "Benvenuto in robocam\n\nPosiziona la videocamera di fronte a te con una buona illuminazione uniforme. Il viso dovrebbe occupare circa un terzo della larghezza dell'anteprima.\n\nQuesta procedura misura i movimenti comodi della testa per regolare la velocità del puntatore. Il controllo è in pausa durante la calibrazione."
        }
        "Frame your face\n\nLook at the live camera preview. Center your face until a tracking rectangle appears.\n\nNext, you will have 8 seconds to move your head slowly LEFT and RIGHT over a comfortable range. Keep your shoulders still." => {
            "Inquadra il viso\n\nGuarda l'anteprima della videocamera. Centra il viso finché appare un rettangolo di rilevamento.\n\nPoi avrai 8 secondi per muovere lentamente la testa a SINISTRA e DESTRA, senza sforzarti. Tieni ferme le spalle."
        }
        "Horizontal calibration\n\nMove your head slowly LEFT and RIGHT over a comfortable range. Do not force the movement.\n\nWait until the measurement completes. Next you will move UP and DOWN for 8 seconds." => {
            "Calibrazione orizzontale\n\nMuovi lentamente la testa a SINISTRA e DESTRA, senza sforzarti.\n\nAttendi il termine della misurazione. Poi muoverai la testa in ALTO e in BASSO per 8 secondi."
        }
        "Vertical calibration\n\nMove your head slowly UP and DOWN over a comfortable range.\n\nWait until the measurement completes, then choose Next to review your settings." => {
            "Calibrazione verticale\n\nMuovi lentamente la testa in ALTO e in BASSO, senza sforzarti.\n\nAttendi il termine della misurazione, poi scegli Avanti per verificare le impostazioni."
        }
        "Ready\n\nYour pointer speeds have been adjusted.\n\nPress F11 to enable head control. Move your head to move the pointer; hold still to click. Select different clicks on the top bar.\n\nYou can tune speed, smoothing and dwell time in Configuration at any time." => {
            "Tutto pronto\n\nLe velocità del puntatore sono state regolate.\n\nPremi F11 per attivare il controllo. Muovi la testa per spostare il puntatore; resta fermo per fare clic. Scegli i diversi clic nella barra in alto.\n\nPuoi regolare velocità, fluidità e durata della sosta in Configurazione."
        }
        "Wait until your face is detected in the preview before starting." => {
            "Prima di iniziare, attendi che il viso venga rilevato nell'anteprima."
        }
        "Not enough movement was measured.\n\nCheck that your face is visible and well lit. Choose Retry and move your head slowly over a comfortable range." => {
            "Movimento rilevato insufficiente.\n\nVerifica che il viso sia visibile e ben illuminato. Scegli Riprova e muovi lentamente la testa, senza sforzarti."
        }
        "Measurement complete.\n\nChoose Next to continue." => {
            "Misurazione completata.\n\nScegli Avanti per continuare."
        }
        "Could not register the robocam window." => {
            "Impossibile registrare la finestra di robocam."
        }
        "Could not create the robocam window." => "Impossibile creare la finestra di robocam.",
        "Could not open the keyboard. Configure its executable path in General." => {
            "Impossibile aprire la tastiera. Configura il percorso del programma in Generale."
        }
        "Choose a different hotkey for each action." => "Scegli un tasto diverso per ogni azione.",
        "Enter an on-screen keyboard executable." => {
            "Inserisci il programma della tastiera su schermo."
        }
        "Enter a profile name of 1–64 characters in the profile box." => {
            "Inserisci un nome da 1 a 64 caratteri nella casella del profilo."
        }
        "That profile already exists. Enter a new name, then choose Save as." => {
            "Il profilo esiste già. Inserisci un nuovo nome e scegli Salva come."
        }
        "Keep at least one profile." => "Deve rimanere almeno un profilo.",
        "Choose a camera index of zero or greater." => {
            "Scegli un indice della videocamera maggiore o uguale a zero."
        }
        "The camera stopped delivering frames. Reconnect it and restart the camera." => {
            "La videocamera non invia più immagini. Ricollegala e riavviala."
        }
        "Camera stopped" => "Videocamera arrestata",
        "robocam 0.1.0\nHead-controlled mouse for Windows 11" => {
            "robocam 0.1.0\nMouse controllato con la testa per Windows 11"
        }
        "F11: enable / pause    F10: center pointer    F9: workspace limits" => {
            "F11: attiva / pausa    F10: centra puntatore    F9: limiti area"
        }
        "Center your face in the preview, enable, then move your head. Stop over a target to click. Select an action on the click bar; select it again to lock it." => {
            "Centra il viso nell'anteprima, attiva il controllo e muovi la testa. Fermati su un elemento per fare clic. Scegli un'azione nella barra dei clic; riselezionala per fissarla."
        }
        "Drag modifies the selected single-click button: stop to hold, move, then stop again to release. Scroll: stop to enter, move your head to scroll, stop again to leave. No click disables desktop clicks; the click bar remains accessible." => {
            "Trascina usa il pulsante del clic singolo selezionato: fermati per tenerlo premuto, muoviti e fermati di nuovo per rilasciarlo. Scorri: fermati per iniziare, muovi la testa per scorrere e fermati per terminare. No clic disattiva i clic sul desktop; la barra resta accessibile."
        }
        "To track a manual region, turn off automatic face location and drag a rectangle in the preview." => {
            "Per seguire un'area manualmente, disattiva il rilevamento automatico del viso e traccia un rettangolo nell'anteprima."
        }
        "Includes GPL-3.0-or-later contributions by Cesar Mauri Loba and contributors. Camera processing stays on this computer." => {
            "Include contributi GPL-3.0-or-later di Cesar Mauri Loba e collaboratori. Le immagini della videocamera vengono elaborate su questo computer."
        }
        "See README and THIRD_PARTY_NOTICES in the installation directory." => {
            "Consulta README e THIRD_PARTY_NOTICES nella cartella di installazione."
        }
        _ => return None,
    };
    Some(translated)
}

fn formatted_italian(text: &str) -> Cow<'_, str> {
    // The status bar is formatted before it reaches the native display helper.
    // Translate only known wrappers; diagnostic details and user data stay intact.
    for (prefix, replacement) in [
        ("Tracking · ", "Controllo attivo · "),
        (
            "Clicks paused — dwell on Play to resume · ",
            "Click in pausa — soffermati su Play · ",
        ),
        (
            "Paused · F11 to enable · ",
            "In pausa · F11 per attivare · ",
        ),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return Cow::Owned(format!(
                "{replacement}{}",
                rest.replace(" points", " punti")
            ));
        }
    }
    for (prefix, replacement) in [
        (
            "Could not save settings: ",
            "Impossibile salvare le impostazioni: ",
        ),
        (
            "Could not change startup setting: ",
            "Impossibile modificare l'avvio automatico: ",
        ),
        (
            "Could not check for updates: ",
            "Impossibile cercare aggiornamenti: ",
        ),
        (
            "Could not start camera worker: ",
            "Impossibile avviare la videocamera: ",
        ),
        (
            "Could not start face detection: ",
            "Impossibile avviare il rilevamento del viso: ",
        ),
        (
            "Could not load the embedded face detector: ",
            "Impossibile caricare il rilevatore del viso integrato: ",
        ),
        (
            "Camera capture failed: ",
            "Acquisizione dalla videocamera non riuscita: ",
        ),
        (
            "Camera image processing failed: ",
            "Elaborazione delle immagini non riuscita: ",
        ),
        (
            "Face detection failed: ",
            "Rilevamento del viso non riuscito: ",
        ),
        ("Windows error ", "Errore di Windows "),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return Cow::Owned(format!("{replacement}{rest}"));
        }
    }
    if let Some(rest) = text.strip_prefix("Hotkey ")
        && let Some(key) =
            rest.strip_suffix(" is already in use. Choose another key in Configuration.")
    {
        return Cow::Owned(format!(
            "Il tasto {key} è già in uso. Scegli un altro tasto in Configurazione."
        ));
    }
    if let Some(rest) = text.strip_prefix("Enter a number between ")
        && let Some((minimum, rest)) = rest.split_once(" and ")
        && let Some((maximum, value)) = rest.split_once(" (field contains ")
    {
        return Cow::Owned(format!(
            "Inserisci un numero tra {minimum} e {maximum} (contenuto del campo: {value}"
        ));
    }
    if let Some(rest) = text.strip_prefix("Could not open camera ")
        && let Some((index, detail)) = rest.split_once(". Check that it is connected, close other camera apps, and allow desktop camera access in Windows Settings > Privacy & security > Camera.") {
            return Cow::Owned(format!("Impossibile aprire la videocamera {index}. Verifica il collegamento, chiudi le altre app che la usano e consenti l'accesso alle app desktop in Impostazioni di Windows > Privacy e sicurezza > Fotocamera.{detail}"));
    }
    if let Some(rest) = text.strip_prefix("robocam — update ")
        && let Some(version) = rest.strip_suffix(" available")
    {
        return Cow::Owned(format!("robocam — aggiornamento {version} disponibile"));
    }
    if let Some(rest) = text.strip_prefix("robocam ")
        && let Some((version, url)) = rest.split_once(" is available.\n\n")
    {
        return Cow::Owned(format!("È disponibile robocam {version}.\n\n{url}"));
    }
    // Help is assembled from paragraphs before display. Retain line breaks and
    // translate each recognized paragraph (or individual line) independently.
    if text.contains('\n') {
        let mut changed = false;
        let mut output = String::with_capacity(text.len());
        for paragraph in text.split_inclusive("\n\n") {
            let content = paragraph.trim_end_matches('\n');
            if let Some(translated) = static_italian(content) {
                changed = true;
                output.push_str(translated);
            } else if content.contains('\n') {
                for line in content.split_inclusive('\n') {
                    let content = line.trim_end_matches('\n');
                    let translated = italian(content);
                    changed |= translated.as_ref() != content;
                    output.push_str(&translated);
                    output.push_str(&line[content.len()..]);
                }
            } else {
                let translated = italian(content);
                changed |= translated.as_ref() != content;
                output.push_str(&translated);
            }
            output.push_str(&paragraph[content.len()..]);
        }
        if changed {
            return Cow::Owned(output);
        }
    }
    Cow::Borrowed(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_selection_and_unknown_values_are_preserved() {
        set_language("en");
        assert_eq!(translate("Left click"), "Left click");
        set_language("it-IT");
        assert_eq!(translate("Left click"), "Clic sinistro");
        assert_eq!(translate("EDIT"), "EDIT");
        assert_eq!(
            translate(r"C:\Tools\keyboard.exe"),
            r"C:\Tools\keyboard.exe"
        );
        set_language("Italiano");
        assert_eq!(translate("Cancel"), "Annulla");
        set_language("unknown");
        assert_eq!(translate("Cancel"), "Cancel");
    }

    #[test]
    fn formatted_status_preserves_numbers_and_errors_preserve_details() {
        assert_eq!(
            italian("Tracking · 29 fps · 20 points"),
            "Controllo attivo · 29 fps · 20 punti"
        );
        assert_eq!(
            italian("Camera capture failed: E_FAIL 0x80004005"),
            "Acquisizione dalla videocamera non riuscita: E_FAIL 0x80004005"
        );
        assert_eq!(
            italian("Hotkey F12 is already in use. Choose another key in Configuration."),
            "Il tasto F12 è già in uso. Scegli un altro tasto in Configurazione."
        );
    }

    #[test]
    fn multiline_help_translates_without_losing_layout_or_unknown_text() {
        let text = "robocam 0.1.0\nHead-controlled mouse for Windows 11\n\nF11: enable / pause    F10: center pointer    F9: workspace limits\nUntranslated diagnostic\n\nSee README and THIRD_PARTY_NOTICES in the installation directory.";
        let translated = italian(text);
        assert!(translated.contains("Mouse controllato con la testa per Windows 11\n\n"));
        assert!(translated.contains("F11: attiva / pausa"));
        assert!(translated.contains("\nUntranslated diagnostic\n\n"));
        assert!(translated.ends_with("nella cartella di installazione."));
    }
}
