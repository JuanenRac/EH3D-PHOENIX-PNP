// EH3D-PHOENIX-PNP - what the simulated machine says it did, in the eight languages.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The events of the simulated machine (a move, a pick, a vacuum reading, a decision of the person) are kept as what happened and the values of it, not as a finished sentence, so that the
//! console of the window can say them in its language. The English row is the text that reports, saved files and the command line have always had.

use crate::error::Lang;
use crate::msg8::{fill, pick};

macro_rules! events {
    ($($name:ident => [$($row:expr),+ $(,)?]),+ $(,)?) => {
        /// What happened. The places `{0}`, `{1}`... of its row are filled with the values, in order.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum What {
            $($name),+
        }

        impl What {
            pub const ALL: &'static [What] = &[$(What::$name),+];

            fn row(self) -> [&'static str; 8] {
                match self {
                    $(What::$name => [$($row),+]),+
                }
            }
        }
    };
}

events! {
    TipMounted => [
        "nozzle tip '{0}' mounted", "punta '{0}' montada", "Düsenspitze '{0}' eingesetzt", "buse '{0}' montée",
        "punta '{0}' montata", "ノズルチップ「{0}」を装着しました", "已装上吸嘴头“{0}”", "ponta '{0}' montada"
    ],
    TipChanged => [
        "nozzle tip changed to '{0}'", "punta cambiada a '{0}'", "Düsenspitze gewechselt zu '{0}'", "buse changée pour '{0}'",
        "punta cambiata in '{0}'", "ノズルチップを「{0}」に交換しました", "吸嘴头已换为“{0}”", "ponta mudada para '{0}'"
    ],
    StillHolds => [
        "nozzle {0} still holds {1} after the place", "la boquilla {0} sigue con {1} después de colocar",
        "Düse {0} hält {1} nach dem Setzen noch", "la buse {0} tient encore {1} après la pose",
        "l'ugello {0} tiene ancora {1} dopo il posizionamento", "配置後もノズル {0} が {1} を保持しています",
        "贴装后吸嘴 {0} 仍吸着 {1}", "o bico {0} continua com {1} depois de colocar"
    ],
    MoveTo => [
        "move to X {0} Y {1}", "movimiento a X {0} Y {1}", "Fahrt nach X {0} Y {1}", "déplacement vers X {0} Y {1}",
        "spostamento a X {0} Y {1}", "X {0} Y {1} へ移動", "移动到 X {0} Y {1}", "deslocação para X {0} Y {1}"
    ],
    WentPast => [
        "went past the place by X {0} Y {1} and came back from the positive side",
        "pasó el sitio en X {0} Y {1} y volvió desde el lado positivo",
        "über die Stelle um X {0} Y {1} hinaus gefahren und von der positiven Seite zurück",
        "a dépassé l'emplacement de X {0} Y {1} et est revenu par le côté positif",
        "ha superato il punto di X {0} Y {1} ed è tornato dal lato positivo",
        "目標を X {0} Y {1} 行き過ぎ、正の側から戻りました",
        "越过目标 X {0} Y {1}，再从正方向返回",
        "passou o local em X {0} Y {1} e voltou pelo lado positivo"
    ],
    EndedShort => [
        "the move ended X {0} Y {1} from the place: an axis reversed and gave by its slack",
        "el movimiento acabó a X {0} Y {1} del sitio: un eje invirtió el sentido y cedió su holgura",
        "die Fahrt endete X {0} Y {1} neben der Stelle: eine Achse kehrte um und gab ihr Spiel nach",
        "le déplacement s'est arrêté à X {0} Y {1} de l'emplacement : un axe a changé de sens et a pris son jeu",
        "lo spostamento è finito a X {0} Y {1} dal punto: un asse ha invertito il verso e ha ceduto il suo gioco",
        "目標から X {0} Y {1} ずれて停止しました：軸が反転し、バックラッシュ分だけ遅れました",
        "移动结束时偏离目标 X {0} Y {1}：某轴换向，出现了间隙",
        "o movimento acabou a X {0} Y {1} do local: um eixo inverteu o sentido e cedeu a folga"
    ],
    PickFailed => [
        "pick of {0} failed", "falló la recogida de {0}", "Aufnehmen von {0} fehlgeschlagen", "la prise de {0} a échoué",
        "prelievo di {0} non riuscito", "{0} の吸着に失敗しました", "拾取 {0} 失败", "falhou a recolha de {0}"
    ],
    Picked => [
        "nozzle {0} picked {1}", "la boquilla {0} recogió {1}", "Düse {0} hat {1} aufgenommen", "la buse {0} a pris {1}",
        "l'ugello {0} ha prelevato {1}", "ノズル {0} が {1} を吸着しました", "吸嘴 {0} 拾取了 {1}", "o bico {0} recolheu {1}"
    ],
    PlaceFailed => [
        "place of {0} failed", "falló la colocación de {0}", "Setzen von {0} fehlgeschlagen", "la pose de {0} a échoué",
        "posizionamento di {0} non riuscito", "{0} の配置に失敗しました", "贴装 {0} 失败", "falhou a colocação de {0}"
    ],
    Placed => [
        "nozzle {0} placed {1}", "la boquilla {0} colocó {1}", "Düse {0} hat {1} gesetzt", "la buse {0} a posé {1}",
        "l'ugello {0} ha posizionato {1}", "ノズル {0} が {1} を配置しました", "吸嘴 {0} 贴装了 {1}", "o bico {0} colocou {1}"
    ],
    Discarded => [
        "nozzle {0} discarded {1}", "la boquilla {0} descartó {1}", "Düse {0} hat {1} verworfen", "la buse {0} a rejeté {1}",
        "l'ugello {0} ha scartato {1}", "ノズル {0} が {1} を廃棄しました", "吸嘴 {0} 丢弃了 {1}", "o bico {0} rejeitou {1}"
    ],
    ActuatorOn => [
        "actuator {0} switched on", "actuador {0} activado", "Aktor {0} eingeschaltet", "actionneur {0} activé",
        "attuatore {0} acceso", "アクチュエータ {0} をオンにしました", "执行器 {0} 已打开", "atuador {0} ligado"
    ],
    ActuatorOff => [
        "actuator {0} switched off", "actuador {0} desactivado", "Aktor {0} ausgeschaltet", "actionneur {0} désactivé",
        "attuatore {0} spento", "アクチュエータ {0} をオフにしました", "执行器 {0} 已关闭", "atuador {0} desligado"
    ],
    LeftOut => [
        "{0} left out by the person", "{0} omitida por la persona", "{0} von der Person ausgelassen", "{0} ignorée par la personne",
        "{0} saltata dalla persona", "{0} は担当者がスキップしました", "{0} 已由操作者跳过", "{0} omitida pela pessoa"
    ],
    PutAside => [
        "{0} put aside by the person", "{0} apartada por la persona", "{0} von der Person zurückgestellt", "{0} mise de côté par la personne",
        "{0} messa da parte dalla persona", "{0} は担当者が後回しにしました", "{0} 已由操作者搁置", "{0} posta de lado pela pessoa"
    ],
    FeederOffByPerson => [
        "feeder {0} switched off by the person", "alimentador {0} desactivado por la persona", "Feeder {0} von der Person abgeschaltet",
        "chargeur {0} désactivé par la personne", "alimentatore {0} spento dalla persona", "フィーダー {0} は担当者が停止しました",
        "供料器 {0} 已由操作者关闭", "alimentador {0} desligado pela pessoa"
    ],
    FeederOffFaults => [
        "feeder {0} switched off after {1} faults", "alimentador {0} desactivado tras {1} fallos",
        "Feeder {0} nach {1} Fehlern abgeschaltet", "chargeur {0} désactivé après {1} défauts",
        "alimentatore {0} spento dopo {1} guasti", "フィーダー {0} は {1} 回の失敗で停止しました",
        "供料器 {0} 在 {1} 次故障后关闭", "alimentador {0} desligado após {1} falhas"
    ],
    FeederWaits => [
        "feeder {0} has {1} faults among its last {2} picks: the job waits for the person",
        "el alimentador {0} tiene {1} fallos en sus últimas {2} recogidas: el trabajo espera a la persona",
        "Feeder {0} hat {1} Fehler in den letzten {2} Aufnahmen: der Auftrag wartet auf die Person",
        "le chargeur {0} a {1} défauts parmi ses {2} dernières prises : le travail attend la personne",
        "l'alimentatore {0} ha {1} guasti negli ultimi {2} prelievi: il lavoro aspetta la persona",
        "フィーダー {0} は直近 {2} 回の吸着のうち {1} 回失敗しました。ジョブは担当者を待ちます",
        "供料器 {0} 最近 {2} 次拾取中有 {1} 次故障：作业等待操作者",
        "o alimentador {0} tem {1} falhas nas últimas {2} recolhas: o trabalho espera pela pessoa"
    ],
    VacuumHeld => [
        "vacuum of nozzle {0}: {1} kPa, {2} is held", "vacío de la boquilla {0}: {1} kPa, {2} está sujeta",
        "Vakuum der Düse {0}: {1} kPa, {2} wird gehalten", "vide de la buse {0} : {1} kPa, {2} est tenue",
        "vuoto dell'ugello {0}: {1} kPa, {2} è trattenuta", "ノズル {0} の真空：{1} kPa、{2} を保持しています",
        "吸嘴 {0} 真空：{1} kPa，已吸住 {2}", "vácuo do bico {0}: {1} kPa, {2} está presa"
    ],
    VacuumNoPart => [
        "vacuum of nozzle {0}: {1} kPa, no part on the nozzle", "vacío de la boquilla {0}: {1} kPa, no hay pieza en la boquilla",
        "Vakuum der Düse {0}: {1} kPa, kein Teil an der Düse", "vide de la buse {0} : {1} kPa, aucune pièce sur la buse",
        "vuoto dell'ugello {0}: {1} kPa, nessuna parte sull'ugello", "ノズル {0} の真空：{1} kPa、ノズルに部品がありません",
        "吸嘴 {0} 真空：{1} kPa，吸嘴上没有零件", "vácuo do bico {0}: {1} kPa, não há peça no bico"
    ],
    VacuumLetGo => [
        "vacuum of nozzle {0}: {1} kPa, {2} let go", "vacío de la boquilla {0}: {1} kPa, {2} se soltó",
        "Vakuum der Düse {0}: {1} kPa, {2} losgelassen", "vide de la buse {0} : {1} kPa, {2} s'est détachée",
        "vuoto dell'ugello {0}: {1} kPa, {2} rilasciata", "ノズル {0} の真空：{1} kPa、{2} を離しました",
        "吸嘴 {0} 真空：{1} kPa，已释放 {2}", "vácuo do bico {0}: {1} kPa, {2} soltou-se"
    ],
    VacuumStuck => [
        "vacuum of nozzle {0}: {1} kPa, {2} is still on the nozzle", "vacío de la boquilla {0}: {1} kPa, {2} sigue en la boquilla",
        "Vakuum der Düse {0}: {1} kPa, {2} hängt noch an der Düse", "vide de la buse {0} : {1} kPa, {2} est encore sur la buse",
        "vuoto dell'ugello {0}: {1} kPa, {2} è ancora sull'ugello", "ノズル {0} の真空：{1} kPa、{2} がまだノズルに付いています",
        "吸嘴 {0} 真空：{1} kPa，{2} 仍在吸嘴上", "vácuo do bico {0}: {1} kPa, {2} continua no bico"
    ],
    NoReading => [
        "the camera gave no reading of the part on nozzle {0}", "la cámara no dio lectura de la pieza de la boquilla {0}",
        "die Kamera gab keine Messung des Teils an Düse {0}", "la caméra n'a donné aucune mesure de la pièce sur la buse {0}",
        "la fotocamera non ha dato alcuna lettura della parte sull'ugello {0}", "カメラはノズル {0} の部品を読み取れませんでした",
        "相机未能读取吸嘴 {0} 上的零件", "a câmara não deu leitura da peça do bico {0}"
    ],
    VisionNothing => [
        "the vision found no part on nozzle {0}: {1}", "la visión no encontró pieza en la boquilla {0}: {1}",
        "die Bildverarbeitung fand kein Teil an Düse {0}: {1}", "la vision n'a trouvé aucune pièce sur la buse {0} : {1}",
        "la visione non ha trovato alcuna parte sull'ugello {0}: {1}", "画像処理はノズル {0} に部品を見つけられませんでした：{1}",
        "视觉未在吸嘴 {0} 上找到零件：{1}", "a visão não encontrou peça no bico {0}: {1}"
    ],
    LooksAgain => [
        "the part on nozzle {0} is {1} mm from the nozzle: the camera looks again",
        "la pieza de la boquilla {0} está a {1} mm de la boquilla: la cámara vuelve a mirar",
        "das Teil an Düse {0} liegt {1} mm neben der Düse: die Kamera sieht noch einmal hin",
        "la pièce sur la buse {0} est à {1} mm de la buse : la caméra regarde à nouveau",
        "la parte sull'ugello {0} è a {1} mm dall'ugello: la fotocamera guarda di nuovo",
        "ノズル {0} の部品はノズルから {1} mm ずれています。カメラで再確認します",
        "吸嘴 {0} 上的零件偏离吸嘴 {1} mm：相机再看一次",
        "a peça do bico {0} está a {1} mm do bico: a câmara volta a olhar"
    ],
    ThrownAway => [
        "the part on nozzle {0} is {1} mm from the nozzle: thrown away",
        "la pieza de la boquilla {0} está a {1} mm de la boquilla: se desecha",
        "das Teil an Düse {0} liegt {1} mm neben der Düse: verworfen",
        "la pièce sur la buse {0} est à {1} mm de la buse : rejetée",
        "la parte sull'ugello {0} è a {1} mm dall'ugello: scartata",
        "ノズル {0} の部品はノズルから {1} mm ずれています。廃棄します",
        "吸嘴 {0} 上的零件偏离吸嘴 {1} mm：已丢弃",
        "a peça do bico {0} está a {1} mm do bico: é rejeitada"
    ],
    Look => [
        "look {0} at nozzle {1}: X {2} Y {3} mm, turned {4} degrees", "mirada {0} a la boquilla {1}: X {2} Y {3} mm, girada {4} grados",
        "Blick {0} auf Düse {1}: X {2} Y {3} mm, um {4} Grad gedreht", "regard {0} sur la buse {1} : X {2} Y {3} mm, tournée de {4} degrés",
        "lettura {0} dell'ugello {1}: X {2} Y {3} mm, ruotata di {4} gradi", "ノズル {1} の確認 {0} 回目：X {2} Y {3} mm、回転 {4} 度",
        "第 {0} 次查看吸嘴 {1}：X {2} Y {3} mm，旋转 {4} 度", "observação {0} do bico {1}: X {2} Y {3} mm, rodada {4} graus"
    ],
    LearnedOffset => [
        "nozzle {0} places {1} with the learned offset X {2} Y {3} mm taken out",
        "la boquilla {0} coloca {1} descontando el desplazamiento aprendido X {2} Y {3} mm",
        "Düse {0} setzt {1} abzüglich des gelernten Versatzes X {2} Y {3} mm",
        "la buse {0} pose {1} en retirant le décalage appris X {2} Y {3} mm",
        "l'ugello {0} posiziona {1} togliendo lo scostamento appreso X {2} Y {3} mm",
        "ノズル {0} は学習した偏差 X {2} Y {3} mm を差し引いて {1} を配置します",
        "吸嘴 {0} 扣除已学习的偏移 X {2} Y {3} mm 后贴装 {1}",
        "o bico {0} coloca {1} descontando o desvio aprendido X {2} Y {3} mm"
    ],
    PlacesTurned => [
        "nozzle {0} places {1} turned {2} degrees", "la boquilla {0} coloca {1} girada {2} grados",
        "Düse {0} setzt {1} um {2} Grad gedreht", "la buse {0} pose {1} tournée de {2} degrés",
        "l'ugello {0} posiziona {1} ruotata di {2} gradi", "ノズル {0} は {1} を {2} 度回転して配置します",
        "吸嘴 {0} 将 {1} 旋转 {2} 度后贴装", "o bico {0} coloca {1} rodada {2} graus"
    ],
}

impl What {
    /// The sentence in a language, with its values.
    pub fn line(self, lang: Lang, values: &[String]) -> String {
        const PLACES: [&str; 5] = ["0", "1", "2", "3", "4"];
        let pairs: Vec<(&str, String)> = PLACES.into_iter().zip(values.iter().cloned()).collect();
        fill(pick(lang, &self.row()), &pairs)
    }

    /// How many values the sentence takes.
    pub fn places(self) -> usize {
        let english = self.row()[0];
        (0..5)
            .take_while(|i| english.contains(&format!("{{{i}}}")))
            .count()
    }
}
