interface CharacterSummary {
  character_id: string;
  character_name: string;
}

interface HeaderControlsProps {
  onSelectVideo: () => void;
  isScanning: boolean;
  myCharacter: string;
  setMyCharacter: (c: string) => void;
  opponentCharacter: string;
  setOpponentCharacter: (c: string) => void;
  controlType: string;
  setControlType: (c: string) => void;
  deviceType: string;
  setDeviceType: (d: string) => void;
  characters: CharacterSummary[];
  charactersLoading: boolean;
}

export function HeaderControls(props: HeaderControlsProps) {
  const {
    onSelectVideo,
    isScanning,
    myCharacter,
    setMyCharacter,
    opponentCharacter,
    setOpponentCharacter,
    controlType,
    setControlType,
    deviceType,
    setDeviceType,
    characters,
    charactersLoading,
  } = props;

  async function selectVideo() {
    onSelectVideo();
  }

  return (
    <header className="app-header">
      <div className="header-title">
        <h1>ST6 Trainer</h1>
        <div className="header-subtitle">Street Fighter 6 対戦分析・トレーニング支援</div>
      </div>
      <div className="header-controls">
        <button onClick={selectVideo} disabled={isScanning}>
          🎬 動画読込
        </button>
      </div>
      <div className="header-filters">
        <div className="filter-group">
          <label>自キャラ</label>
          <select
            value={myCharacter}
            onChange={(e) => setMyCharacter(e.target.value)}
            disabled={charactersLoading}
          >
            <option value="">選択してください</option>
            {characters.map((c) => (
              <option key={c.character_id} value={c.character_id}>
                {c.character_name} ({c.character_id})
              </option>
            ))}
          </select>
        </div>
        <div className="filter-group">
          <label>敵キャラ</label>
          <select
            value={opponentCharacter}
            onChange={(e) => setOpponentCharacter(e.target.value)}
            disabled={charactersLoading}
          >
            <option value="">選択してください</option>
            {characters.map((c) => (
              <option key={c.character_id} value={c.character_id}>
                {c.character_name} ({c.character_id})
              </option>
            ))}
          </select>
        </div>
        <div className="filter-group">
          <label>操作系</label>
          <select value={controlType} onChange={(e) => setControlType(e.target.value)}>
            <option value="classic">クラシック</option>
            <option value="modern">モダン</option>
            <option value="modern_manual">モダン(マニュアル)</option>
          </select>
        </div>
        <div className="filter-group">
          <label>デバイス</label>
          <select value={deviceType} onChange={(e) => setDeviceType(e.target.value)}>
            <option value="pad">パッド</option>
            <option value="arcade_stick">アケコン</option>
            <option value="leverless">レバーレス</option>
          </select>
        </div>
      </div>
    </header>
  );
}
