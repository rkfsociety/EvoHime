use super::*;

/// Запросы, которыми внутренние компоненты Core вызывают операции runtime и
/// передают результат через одноразовый канал ответа. Этот тип не является
/// транспортным IPC-сообщением: внешние клиенты используют версии протокола
/// из `evohime-desktop-ipc`.
pub enum CoreCommand {
    /// Запускает задачу с заданной инструкцией и необязательными настройками
    /// workspace и выбора маршрута.
    StartTask {
        /// Идентификатор новой задачи.
        task_id: String,
        /// Инструкция, передаваемая агенту.
        prompt: String,
        /// Корень workspace, в котором выполняется задача, если он задан.
        workspace_root: Option<PathBuf>,
        /// Подсказка для выбора модели/маршрута; runtime проверяет её сам.
        preferred_route_hint: Option<String>,
    },
    /// Останавливает выполняемую задачу по её идентификатору.
    StopTask {
        /// Идентификатор останавливаемой задачи.
        task_id: String,
    },
    /// Эпизод постоянного слушания закрылся: разобрать его в кандидатов
    /// памяти (04.6). Ответа нет намеренно — извлечение идёт после того, как
    /// эпизод уже закрыт, и не должно никого ждать.
    ExtractAmbientMemory {
        /// Идентификатор уже закрытого эпизода прослушивания.
        episode_id: String,
    },
    /// Передаёт решение пользователя по ожидающему выбора маршрута.
    ResolveRoutingDecision {
        /// Идентификатор ожидающего trace/решения.
        trace_id: String,
        /// `true`, если пользователь одобрил предложенный маршрут.
        approve: bool,
        /// Канал, в который обработчик отправляет результат разрешения.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Создаёт проект с указанным workspace и необязательной исходной ссылкой.
    CreateProject {
        /// Идентификатор клиента, отправившего запрос.
        client_id: String,
        /// Идентификатор запроса для сопоставления ответа и повторов.
        request_id: String,
        /// Хэш команды для проверки корректности повторного запроса.
        command_hash: String,
        /// Идентификатор создаваемого проекта.
        project_id: String,
        /// Отображаемое название проекта.
        title: String,
        /// Локальный путь workspace проекта.
        workspace_path: String,
        /// Необязательная ссылка на исходный объект или ref.
        source_ref: Option<String>,
        /// Канал результата создания проекта.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Создаёт задачу из проверенной записи work item.
    CreateTask {
        /// Идентификатор клиента, отправившего запрос.
        client_id: String,
        /// Идентификатор запроса для сопоставления ответа и повторов.
        request_id: String,
        /// Хэш команды для проверки корректности повторного запроса.
        command_hash: String,
        /// Содержимое и атрибуты новой задачи.
        item: WorkItemRecord,
        /// Канал результата создания задачи.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Меняет состояние задачи при совпадении ожидаемой версии записи.
    UpdateTaskStatus {
        /// Идентификатор клиента, отправившего запрос.
        client_id: String,
        /// Идентификатор запроса для сопоставления ответа и повторов.
        request_id: String,
        /// Хэш команды для проверки корректности повторного запроса.
        command_hash: String,
        /// Идентификатор изменяемой задачи.
        task_id: String,
        /// Версия задачи, которую вызывающий код прочитал перед изменением.
        expected_version: i64,
        /// Новое значение статуса; допустимость проверяется обработчиком.
        status: String,
        /// Канал результата обновления статуса.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Добавляет направленную связь между двумя задачами.
    AddTaskEdge {
        /// Идентификатор клиента, отправившего запрос.
        client_id: String,
        /// Идентификатор запроса для сопоставления ответа и повторов.
        request_id: String,
        /// Хэш команды для проверки корректности повторного запроса.
        command_hash: String,
        /// Идентификатор исходной задачи связи.
        from_task_id: String,
        /// Идентификатор целевой задачи связи.
        to_task_id: String,
        /// Вид зависимости между задачами.
        kind: String,
        /// Канал результата добавления связи.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Возвращает граф задач выбранного проекта.
    GetTaskGraph {
        /// Идентификатор проекта, граф которого запрашивается.
        project_id: String,
        /// Канал результата чтения графа.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Возвращает следующую задачу проекта, готовую к выполнению.
    NextReadyTask {
        /// Идентификатор проекта для выбора готовой задачи.
        project_id: String,
        /// Канал результата поиска готовой задачи.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Импортирует PRD как версионированный источник требований проекта.
    ImportPrd {
        /// Идентификатор клиента, отправившего запрос.
        client_id: String,
        /// Идентификатор запроса для сопоставления ответа и повторов.
        request_id: String,
        /// Хэш команды для проверки корректности повторного запроса.
        command_hash: String,
        /// Идентификатор этой операции импорта.
        import_id: String,
        /// Идентификатор проекта, которому принадлежит PRD.
        project_id: String,
        /// Происхождение импортируемого документа.
        origin: String,
        /// Версия PRD, предоставленная источником.
        version: String,
        /// Исходный текст PRD для разбора и сохранения.
        source_text: String,
        /// Канал результата импорта.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Читает последние события истории указанной задачи.
    GetTaskHistory {
        /// Идентификатор задачи, историю которой нужно прочитать.
        task_id: String,
        /// Максимальное число возвращаемых событий.
        limit: usize,
        /// Канал результата чтения истории.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Читает ограниченное текстовое представление контекста задачи.
    GetTaskContext {
        /// Идентификатор проекта-владельца задачи.
        project_id: String,
        /// Идентификатор задачи, контекст которой нужен.
        task_id: String,
        /// Максимальный размер результата в символах.
        max_chars: usize,
        /// Канал результата чтения контекста.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Читает ограниченную спецификацию плана задачи.
    GetTaskPlanSpec {
        /// Идентификатор проекта-владельца задачи.
        project_id: String,
        /// Идентификатор задачи, план которой нужен.
        task_id: String,
        /// Максимальный размер результата в символах.
        max_chars: usize,
        /// Канал результата чтения спецификации плана.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Выполняет допустимую операцию над версионируемым артефактом плана.
    PlanArtifact {
        /// Имя операции, интерпретируемое обработчиком артефактов.
        operation: String,
        /// JSON-данные создаваемого или обновляемого артефакта.
        artifact_json: Vec<u8>,
        /// Идентификатор артефакта.
        artifact_id: String,
        /// Версия артефакта, прочитанная вызывающим кодом.
        expected_version: u64,
        /// Требуемый статус артефакта.
        status: String,
        /// Хэш снимка policy, на котором основано изменение.
        policy_snapshot_hash: String,
        /// Задача, которой принадлежит артефакт, если она задана.
        task_id: Option<String>,
        /// Запуск workflow, связанный с артефактом, если он задан.
        workflow_run_id: Option<String>,
        /// Идентификатор для связи с журналом операции.
        correlation_id: String,
        /// Ключ безопасного повтора команды.
        idempotency_key: String,
        /// Канал результата выполнения операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Создаёт или обновляет контрольную точку состояния workspace.
    WorkspaceStateCheckpoint {
        /// Имя операции над контрольной точкой.
        operation: String,
        /// Идентификатор проекта-владельца workspace.
        project_id: String,
        /// Связанная задача, если checkpoint привязан к задаче.
        task_id: Option<String>,
        /// Идентификатор контрольной точки; `None` позволяет обработчику создать его.
        checkpoint_id: Option<String>,
        /// Сериализованные данные состояния для сохранения или восстановления.
        payload: Vec<u8>,
        /// Ожидаемая версия состояния для защиты от потерянного обновления.
        expected_version: u64,
        /// Ключ, предотвращающий повторное применение одной операции.
        idempotency_key: String,
        /// Канал результата сохранения или восстановления.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Запускает операцию протокола инкрементального изменения.
    IncrementalChangeProtocol {
        /// Имя операции протокола.
        operation: String,
        /// Идентификатор запуска, к которому относится изменение.
        run_id: String,
        /// Сериализованные данные операции.
        payload: Vec<u8>,
        /// Версия протокольной записи, которую ожидает вызывающий код.
        expected_version: u64,
        /// Наблюдённый отпечаток для проверки актуальности workspace.
        observed_fingerprint: String,
        /// Ключ, предотвращающий повторное применение изменения.
        idempotency_key: String,
        /// Канал результата проверки или применения изменения.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Выполняет безопасную операцию над версионируемым файлом workspace.
    RevisionSafeWorkspaceFiles {
        /// Операция чтения или изменения файла.
        operation: String,
        /// Идентификатор проекта-владельца файла.
        project_id: String,
        /// Путь в логическом пространстве workspace.
        logical_path: String,
        /// Новое содержимое файла для операции записи.
        content: Vec<u8>,
        /// Хэш текущего содержимого, обязательный для условного обновления.
        expected_hash: String,
        /// Ключ, предотвращающий повторное применение записи.
        idempotency_key: String,
        /// Канал результата файловой операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Выполняет операцию над изоляцией worktree отдельной задачи.
    TaskWorktreeIsolation {
        /// Операция создания, проверки или завершения изоляции.
        operation: String,
        /// Идентификатор проекта, в котором расположен worktree.
        project_id: String,
        /// Идентификатор задачи, изолируемой в worktree.
        task_id: String,
        /// Идентификатор записи worktree.
        worktree_id: String,
        /// Ветка, выделенная задаче.
        branch: String,
        /// Базовый commit, от которого создана ветка.
        base_commit: String,
        /// Ожидаемая версия записи изоляции.
        expected_version: u64,
        /// Ключ, предотвращающий повторное применение операции.
        idempotency_key: String,
        /// Канал результата операции над worktree.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Применяет операцию над бюджетом ресурсов команды.
    TeamResourceBudget {
        /// Имя операции над бюджетом.
        operation: String,
        /// Пространство-владелец бюджета (например, идентификатор команды).
        owner_scope: String,
        /// Сериализованные параметры операции.
        payload: Vec<u8>,
        /// Ожидаемая версия записи бюджета.
        expected_version: u64,
        /// Ключ, предотвращающий повторное изменение бюджета.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Меняет композицию условий завершения задачи или процесса.
    ComposableTerminationConditions {
        /// Имя операции над условиями завершения.
        operation: String,
        /// Пространство-владелец набора условий.
        owner_scope: String,
        /// Сериализованные параметры операции.
        payload: Vec<u8>,
        /// Ожидаемая версия набора условий.
        expected_version: u64,
        /// Ключ, предотвращающий повторное изменение.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Создаёт или изменяет манифест начальной настройки workspace.
    WorkspaceBootstrapManifest {
        /// Имя операции над манифестом.
        operation: String,
        /// Идентификатор проекта.
        project_id: String,
        /// Идентификатор workspace.
        workspace_id: String,
        /// Сериализованные данные манифеста или операции.
        payload: Vec<u8>,
        /// Ожидаемая версия манифеста.
        expected_version: u64,
        /// Ключ, предотвращающий повторное применение операции.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет policy-правилами координации команды.
    TeamCoordinationPolicies {
        /// Имя операции над policy.
        operation: String,
        /// Идентификатор команды-владельца policy.
        team_id: String,
        /// Сериализованные данные правила или операции.
        payload: Vec<u8>,
        /// Ожидаемая версия policy.
        expected_version: u64,
        /// Ключ, предотвращающий повторное применение изменения.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Проверяет или применяет типизированный пакет передачи работы агенту.
    TypedAgentHandoffContract {
        /// Имя операции с контрактом передачи.
        operation: String,
        /// Идентификатор передачи работы.
        handoff_id: String,
        /// JSON-пакет передачи, проверяемый контрактом.
        packet_json: Vec<u8>,
        /// Идентификатор инициатора операции.
        actor: String,
        /// Причина передачи или отказа.
        reason: String,
        /// Ожидаемая версия записи передачи.
        expected_version: u64,
        /// Ключ, предотвращающий повторную передачу.
        idempotency_key: String,
        /// Канал результата проверки или применения.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет конфигурацией агентов, проверяемой по декларативной схеме.
    SchemaDrivenAgentConfiguration {
        /// Имя операции над конфигурацией.
        operation: String,
        /// Область конфигурации.
        scope: String,
        /// Сериализованная конфигурация или параметры операции.
        payload: Vec<u8>,
        /// Ожидаемая ревизия конфигурации.
        expected_revision: u64,
        /// Ключ, предотвращающий повторное изменение.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет записями библиотеки повторного использования опыта.
    ExperienceReplayLibrary {
        /// Имя операции над библиотекой.
        operation: String,
        /// Область, в которой хранятся записи опыта.
        scope: String,
        /// Идентификатор конкретного пространства библиотеки.
        scope_id: String,
        /// Сериализованная запись опыта или параметры операции.
        payload: Vec<u8>,
        /// Ожидаемая ревизия библиотеки.
        expected_revision: u64,
        /// Ключ, предотвращающий повторное применение операции.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Передаёт операцию в pipeline runtime-вмешательств.
    RuntimeInterventionPipeline {
        /// Вид вмешательства, который обрабатывает runtime pipeline.
        operation: String,
        /// Идентификатор запуска, к которому относится вмешательство.
        run_id: String,
        /// Сериализованные данные конкретной операции.
        payload: Vec<u8>,
        /// Ключ, не позволяющий повторному запросу применить вмешательство ещё раз.
        idempotency_key: String,
        /// Канал сериализованного результата или ошибки обработки.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Передаёт проверку или обновление в контур обратной связи диагностики кода.
    CodeDiagnosticsFeedbackLoop {
        /// Вид операции над feedback-состоянием.
        operation: String,
        /// Идентификатор корня workspace, для которого собрана диагностика.
        workspace_root_id: String,
        /// Сериализованные данные конкретной операции.
        payload: Vec<u8>,
        /// Идентификатор исходного снимка, с которым сравнивается результат.
        baseline_snapshot_id: String,
        /// Ревизия feedback-записи, ожидаемая вызывающим кодом.
        expected_revision: u64,
        /// Ключ, предотвращающий повторное применение одной операции.
        idempotency_key: String,
        /// Канал результата операции или диагностируемой ошибки.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Передаёт операцию в lane ревью кода.
    CodeReviewLane {
        /// Вид операции над review lane.
        operation: String,
        /// Идентификатор ревью.
        review_id: String,
        /// Идентификатор объекта, который проходит ревью.
        target_id: String,
        /// Сериализованные данные операции.
        payload: Vec<u8>,
        /// Ревизия review-записи, прочитанная вызывающим кодом.
        expected_revision: u64,
        /// Ключ для безопасного повтора изменения review-записи.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Передаёт операцию в ансамбль независимых рецензентов.
    MultiReviewerEnsemble {
        /// Вид операции над ансамблем.
        operation: String,
        /// Идентификатор ансамбля рецензентов.
        ensemble_id: String,
        /// Сериализованные входные данные или результат операции.
        payload: Vec<u8>,
        /// Ревизия ансамбля, ожидаемая вызывающим кодом.
        expected_revision: u64,
        /// Ключ, предотвращающий повторное выполнение операции.
        idempotency_key: String,
        /// Канал результата ансамбля.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Передаёт запрос в runtime языковой аналитики.
    LanguageIntelligence {
        /// Вид операции языковой аналитики.
        operation: String,
        /// Идентификатор запроса для сопоставления результата.
        request_id: String,
        /// Сериализованные входные данные запроса.
        payload: Vec<u8>,
        /// Ожидаемая ревизия состояния языковой аналитики.
        expected_revision: u64,
        /// Ключ, предотвращающий повторное выполнение запроса с побочными эффектами.
        idempotency_key: String,
        /// Канал результата анализа или ошибки.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Выполняет операцию над пакетом статического анализа.
    StaticAnalysisPacks {
        /// Вид операции над пакетом.
        operation: String,
        /// Идентификатор пакета правил анализа.
        pack_id: String,
        /// Сериализованные входные данные или параметры операции.
        payload: Vec<u8>,
        /// Ожидаемая ревизия пакета.
        expected_revision: u64,
        /// Ключ, предотвращающий повторное применение изменения.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Загружает или изменяет профиль набора контекста.
    ContextLoadouts {
        /// Вид операции над loadout.
        operation: String,
        /// Идентификатор профиля набора контекста.
        profile_id: String,
        /// Сериализованные данные профиля или операции.
        payload: Vec<u8>,
        /// Ожидаемая ревизия профиля.
        expected_revision: u64,
        /// Ключ, предотвращающий повторное применение операции.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет установкой и источником навыка агента.
    SkillSourceLifecycle {
        /// Вид операции жизненного цикла источника.
        operation: String,
        /// Идентификатор установки навыка.
        installation_id: String,
        /// Сериализованные данные навыка или операции.
        payload: Vec<u8>,
        /// Ожидаемая ревизия записи установки.
        expected_revision: u64,
        /// Ключ, предотвращающий повторное применение операции.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Вызывает операцию фасада capability registry.
    KernelCapabilityFacade {
        /// Операция фасада, например чтение или проверка записи.
        operation: String,
        /// Идентификатор записи capability.
        record_id: String,
        /// Сериализованные входные данные операции.
        payload: Vec<u8>,
        /// Ревизия записи, которую ожидает вызывающий код.
        expected_revision: u64,
        /// Ключ для безопасного повтора изменения.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Выполняет операцию в контуре авторизованных security assessments.
    AuthorizedSecurityAssessment {
        /// Вид операции assessment runtime.
        operation: String,
        /// Идентификатор assessment.
        assessment_id: String,
        /// Сериализованные данные запроса.
        payload: Vec<u8>,
        /// Ожидаемая ревизия assessment.
        expected_revision: u64,
        /// Ключ, предотвращающий повторное выполнение операции.
        idempotency_key: String,
        /// Канал результата операции или отказа policy.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Читает или изменяет описание графа сервисов runtime.
    RuntimeServiceGraph {
        /// Вид операции над графом.
        operation: String,
        /// Идентификатор графа сервисов.
        graph_id: String,
        /// Сериализованные данные операции.
        payload: Vec<u8>,
        /// Ожидаемая ревизия графа.
        expected_revision: u64,
        /// Ключ для распознавания повторного изменения.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет программой агента через agent-program optimizer.
    AgentProgramOptimizer {
        /// Вид операции оптимизации или чтения.
        operation: String,
        /// Идентификатор программы агента.
        program_id: String,
        /// Сериализованные параметры или результат операции.
        payload: Vec<u8>,
        /// Ожидаемая ревизия программы.
        expected_revision: u64,
        /// Ключ для безопасного повтора изменения.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет notebook знаний проекта.
    ProjectKnowledgeNotebook {
        /// Вид операции над notebook.
        operation: String,
        /// Идентификатор notebook.
        notebook_id: String,
        /// Сериализованные данные операции.
        payload: Vec<u8>,
        /// Ожидаемая ревизия notebook.
        expected_revision: u64,
        /// Ключ для распознавания повторного изменения.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Выполняет операцию протокола публикации в Git remote.
    GitRemotePublicationProtocol {
        /// Вид операции публикации.
        operation: String,
        /// Идентификатор протокольной записи публикации.
        protocol_id: String,
        /// Сериализованные данные запроса.
        payload: Vec<u8>,
        /// Ожидаемая ревизия записи протокола.
        expected_revision: u64,
        /// Ключ для предотвращения повторной публикации.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Выполняет операцию над профилем ввода голоса.
    VoiceInputDictation {
        /// Вид операции над диктовкой или её профилем.
        operation: String,
        /// Идентификатор профиля диктовки.
        profile_id: String,
        /// Сериализованные параметры операции.
        payload: Vec<u8>,
        /// Ожидаемая ревизия профиля.
        expected_revision: u64,
        /// Ключ, предотвращающий повторную запись результата.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Запускает или проверяет цикл консолидации опыта.
    OfflineExperienceConsolidation {
        /// Вид операции консолидации.
        operation: String,
        /// Идентификатор цикла консолидации.
        cycle_id: String,
        /// Сериализованные параметры или данные цикла.
        payload: Vec<u8>,
        /// Ожидаемая ревизия состояния цикла.
        expected_revision: u64,
        /// Ключ, предотвращающий повторное применение изменения.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Создаёт или выполняет deterministic review execution plan.
    DeterministicReviewExecutionPlan {
        /// Вид операции над планом ревью.
        operation: String,
        /// Идентификатор плана исполнения.
        plan_id: String,
        /// Сериализованные данные плана или операции.
        payload: Vec<u8>,
        /// Ожидаемая ревизия плана.
        expected_revision: u64,
        /// Ключ для безопасного повтора операции.
        idempotency_key: String,
        /// Канал результата выполнения плана.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Передаёт операцию в лабораторию оптимизации workflow.
    WorkflowOptimizationLab {
        /// Вид операции оптимизации workflow.
        operation: String,
        /// Идентификатор запуска оптимизации.
        run_id: String,
        /// Сериализованные входные данные операции.
        payload: Vec<u8>,
        /// Ожидаемая ревизия записи запуска.
        expected_revision: u64,
        /// Ключ, исключающий повторное применение операции.
        idempotency_key: String,
        /// Канал результата оптимизации.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет подписками и возможностями доставки событий Core.
    CoreTopicSubscriptionEventBus {
        /// Вид операции над подписками или журналом событий.
        operation: String,
        /// Сериализованные параметры операции подписки.
        payload: Vec<u8>,
        /// Capability, требуемая для выполнения операции.
        capability: String,
        /// Ключ, предотвращающий повторное создание/изменение подписки.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Выполняет операцию над графом задач с учётом зависимостей и grants.
    DependencyAwareTaskGraph {
        /// Вид операции над графом.
        operation: String,
        /// Идентификатор графа задач.
        graph_id: String,
        /// Сериализованные данные операции.
        payload: Vec<u8>,
        /// Ожидаемая ревизия графа для защиты от устаревшей записи.
        expected_revision: u64,
        /// Grants, предъявляемые вызывающим компонентом; обработчик проверяет их.
        grants: Vec<String>,
        /// Канал результата проверки или изменения графа.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Регистрирует или запрашивает декларативный компонент агента.
    DeclarativeAgentComponentRegistry {
        /// Вид операции реестра.
        operation: String,
        /// Идентификатор реестровой записи.
        registry_id: String,
        /// Сериализованное описание компонента или запроса.
        payload: Vec<u8>,
        /// Ожидаемая ревизия записи.
        expected_revision: u64,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Читает или обновляет типизированную ссылку контекста.
    TypedContextReferences {
        /// Вид операции над ссылкой.
        operation: String,
        /// Идентификатор ссылки контекста.
        ref_id: String,
        /// Сериализованные данные ссылки или команды.
        payload: Vec<u8>,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Проверяет или изменяет расширение безопасного UI.
    SafeUiExtensionFramework {
        /// Вид операции над расширением.
        operation: String,
        /// Идентификатор расширения.
        extension_id: String,
        /// Сериализованные данные расширения или запроса.
        payload: Vec<u8>,
        /// Ожидаемая ревизия расширения.
        expected_revision: u64,
        /// Канал результата policy-проверки или операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет экземпляром capability workbench с проверкой владельца и grants.
    CapabilityWorkbench {
        /// Вид операции над workbench.
        operation: String,
        /// Идентификатор экземпляра workbench.
        instance_id: String,
        /// Идентификатор владельца экземпляра.
        owner_id: String,
        /// Сериализованные параметры операции.
        payload: Vec<u8>,
        /// Ожидаемая ревизия экземпляра.
        expected_revision: u64,
        /// Grants, которые обработчик должен проверить до операции.
        grants: Vec<String>,
        /// Канал результата операции или отказа авторизации.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Выполняет операцию координатора задач команды.
    TeamCoordinator {
        /// Вид операции координации.
        operation: String,
        /// Идентификатор work item, над которым работает координатор.
        work_item_id: String,
        /// Сериализованные данные или параметры операции.
        payload: Vec<u8>,
        /// Ожидаемая ревизия work item.
        expected_revision: u64,
        /// Ключ для распознавания повторного запроса.
        idempotency_key: String,
        /// Канал результата координации.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Собирает или проверяет инструкции проекта с учётом релевантных путей.
    ProjectInstructionStack {
        /// Вид операции над набором инструкций.
        operation: String,
        /// Корень workspace, к которому относятся инструкции.
        workspace_root: String,
        /// Сериализованные данные инструкции или параметры операции.
        payload: Vec<u8>,
        /// Пути workspace, влияющие на выбор инструкций.
        relevant_paths: Vec<String>,
        /// Ожидаемая ревизия стека инструкций.
        expected_revision: u64,
        /// Ключ для безопасного повтора изменения.
        idempotency_key: String,
        /// Канал результата сборки или проверки.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет именованным набором workspace.
    WorkspaceSets {
        /// Вид операции над набором.
        operation: String,
        /// Идентификатор набора workspace.
        set_id: String,
        /// Сериализованные параметры операции.
        payload: Vec<u8>,
        /// Ожидаемая версия набора.
        expected_version: u64,
        /// Ключ для распознавания повтора.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет связью источника знаний с ролью проекта.
    KnowledgeSourceRegistryProjectRole {
        /// Вид операции над привязкой источника.
        operation: String,
        /// Идентификатор источника знаний.
        source_id: String,
        /// Сериализованные данные роли или параметров операции.
        payload: Vec<u8>,
        /// Ожидаемая версия реестровой записи.
        expected_version: u64,
        /// Ключ для безопасного повтора изменения.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Выполняет операцию моста между локальной задачей и удалённым runtime.
    DurableRemoteTaskBridge {
        /// Вид операции моста.
        operation: String,
        /// Идентификатор удалённой задачи.
        remote_task_id: String,
        /// Сериализованные данные запроса или удалённого результата.
        payload: Vec<u8>,
        /// Ожидаемая версия записи удалённой задачи.
        expected_version: u64,
        /// Ключ, предотвращающий повторную отправку побочного действия.
        idempotency_key: String,
        /// Канал результата операции моста.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Проверяет или изменяет policy сообщения и последующих вмешательств.
    MessageInterventionPolicies {
        /// Вид операции над policy.
        operation: String,
        /// Сериализованные данные policy или контекста проверки.
        payload: Vec<u8>,
        /// Ожидаемая версия policy.
        expected_version: u64,
        /// Ключ для распознавания повторного изменения.
        idempotency_key: String,
        /// Канал результата policy-решения или операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Выполняет одну операцию над пакетом batch-вызовов.
    BatchInvocationRuntime {
        /// Вид операции batch runtime.
        operation: String,
        /// Идентификатор пакета вызовов.
        batch_id: String,
        /// Сериализованные элементы пакета или параметры операции.
        payload: Vec<u8>,
        /// Ожидаемая версия записи пакета.
        expected_version: u64,
        /// Ключ для предотвращения повторного запуска пакета.
        idempotency_key: String,
        /// Канал результата запуска или чтения.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Читает или изменяет policy-aware кэш результатов инструментов.
    PolicyAwareToolResultCache {
        /// Вид операции над кэшем.
        operation: String,
        /// Ключ записи кэша.
        cache_key: String,
        /// Сериализованные данные кэша или параметры операции.
        payload: Vec<u8>,
        /// Ожидаемая версия записи.
        expected_version: u64,
        /// Ключ, предотвращающий повторное изменение записи.
        idempotency_key: String,
        /// Канал результата чтения или изменения кэша.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет маркерами намерений, привязанными к конкретному исходному файлу.
    CodeAnchoredIntentMarkers {
        /// Вид операции над маркером.
        operation: String,
        /// Путь исходного файла, к которому относятся маркеры.
        file_path: String,
        /// Ревизия или отпечаток файла, к которому привязан маркер.
        revision: String,
        /// Сериализованные маркеры либо параметры операции.
        payload: Vec<u8>,
        /// Ключ для распознавания повторной записи.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Выбирает или проверяет маршрут модели для заданного назначения.
    ModelPurposeRouting {
        /// Вид операции выбора маршрута.
        operation: String,
        /// Сериализованные параметры назначения и ограничения маршрута.
        payload: Vec<u8>,
        /// Ожидаемая версия каталога маршрутизации.
        expected_version: u64,
        /// Ключ для безопасного повтора операции выбора/изменения.
        idempotency_key: String,
        /// Канал результата маршрутизации или ошибки policy.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет моделью, установленной в локальном runtime.
    LocalModelRuntimeManager {
        /// Вид операции менеджера локальных моделей.
        operation: String,
        /// Сериализованные параметры локальной модели или операции.
        payload: Vec<u8>,
        /// Ожидаемая версия записи модели.
        expected_version: u64,
        /// Ключ, предотвращающий повторное выполнение операции над моделью.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Создаёт или читает снимок архитектуры workspace.
    ArchitectureSnapshot {
        /// Вид операции над снимком.
        operation: String,
        /// Идентификатор снимка архитектуры.
        snapshot_id: String,
        /// Корень workspace, который описывает снимок.
        workspace_root: String,
        /// Сериализованные данные снимка или параметры создания.
        payload: Vec<u8>,
        /// Ожидаемая версия снимка при условном обновлении.
        expected_version: u64,
        /// Ключ для безопасного повтора операции.
        idempotency_key: String,
        /// Канал результата чтения или создания снимка.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Выполняет операцию над набором изменений Git, принадлежащим агенту.
    AgentGitChangeSets {
        /// Вид операции над change set.
        operation: String,
        /// Идентификатор набора изменений.
        change_set_id: String,
        /// Корень workspace, в котором применён набор изменений.
        workspace_root: String,
        /// Сериализованные данные набора изменений.
        payload: Vec<u8>,
        /// Ожидаемая версия набора.
        expected_version: u64,
        /// Ключ для предотвращения повторного применения изменений.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет pipeline между архитектурным редактором и моделью.
    ArchitectEditorModelPipeline {
        /// Вид операции pipeline.
        operation: String,
        /// Идентификатор pipeline.
        pipeline_id: String,
        /// Сериализованные данные редактора или модели.
        payload: Vec<u8>,
        /// Ожидаемая версия pipeline.
        expected_version: u64,
        /// Ключ, предотвращающий повторное выполнение операции.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет регистрацией и настройкой визуализатора событий.
    EventVisualizerRegistry {
        /// Вид операции реестра визуализаторов.
        operation: String,
        /// Идентификатор визуализатора.
        visualizer_id: String,
        /// Сериализованная конфигурация или параметры операции.
        payload: Vec<u8>,
        /// Ожидаемая версия записи реестра.
        expected_version: u64,
        /// Ключ для распознавания повтора изменения.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет библиотекой reasoning operators.
    ReasoningOperatorLibrary {
        /// Вид операции над библиотекой.
        operation: String,
        /// Идентификатор оператора рассуждений.
        operator_id: String,
        /// Сериализованные данные оператора или команды.
        payload: Vec<u8>,
        /// Ожидаемая версия записи оператора.
        expected_version: u64,
        /// Ключ, предотвращающий повторное изменение библиотеки.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Проверяет или обновляет pipeline ограничений выходного содержимого.
    OutputGuardrailPipeline {
        /// Вид операции output guardrail.
        operation: String,
        /// Идентификатор pipeline ограничений.
        pipeline_id: String,
        /// Сериализованные данные проверки или конфигурации.
        payload: Vec<u8>,
        /// Ожидаемая версия pipeline.
        expected_version: u64,
        /// Ключ, предотвращающий повторное применение изменения.
        idempotency_key: String,
        /// Канал результата проверки или изменения.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Выполняет операцию над реестром настроек пользователя и проекта.
    CustomizationInventory {
        /// Вид операции инвентаризации.
        operation: String,
        /// Идентификатор настраиваемого элемента.
        item_id: String,
        /// Сериализованные данные элемента или запроса.
        payload: Vec<u8>,
        /// Ожидаемая версия записи настройки.
        expected_version: u64,
        /// Ключ для безопасного повтора изменения.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет профилем длительных разрешений, заданных пользователем.
    StandingApprovalProfiles {
        /// Вид операции над профилем разрешений.
        operation: String,
        /// Идентификатор профиля.
        profile_id: String,
        /// Сериализованные условия профиля.
        payload: Vec<u8>,
        /// Ожидаемая версия профиля.
        expected_version: u64,
        /// Ключ, предотвращающий повторное сохранение изменения.
        idempotency_key: String,
        /// Канал результата операции или отказа policy.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Управляет профилем правил принятия approval-решений.
    ApprovalPolicyProfiles {
        /// Вид операции над профилем policy.
        operation: String,
        /// Идентификатор профиля policy.
        profile_id: String,
        /// Сериализованные правила или параметры операции.
        payload: Vec<u8>,
        /// Ожидаемая версия профиля.
        expected_version: u64,
        /// Ключ для распознавания повтора изменения.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Создаёт или читает ветвь, ответвлённую от execution checkpoint.
    CheckpointForking {
        /// Вид операции ветвления.
        operation: String,
        /// Идентификатор запуска-ветви.
        fork_run_id: String,
        /// Сериализованные параметры ветвления или checkpoint.
        payload: Vec<u8>,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Применяет privacy/telemetry policy к заданной категории данных.
    PrivacyTelemetryGovernance {
        /// Вид операции над настройками privacy/telemetry.
        operation: String,
        /// Категория данных или telemetry-события.
        category: String,
        /// Сериализованный контекст policy-проверки или изменения.
        payload: Vec<u8>,
        /// Ожидаемая версия policy.
        expected_version: u64,
        /// Ключ для безопасного повтора изменения.
        idempotency_key: String,
        /// Канал результата policy-проверки или изменения.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Выполняет операцию над адаптером внешнего канала разговора.
    ConversationBridgeAdapters {
        /// Вид операции над подключением.
        operation: String,
        /// Идентификатор подключения или адаптера.
        bridge_id: String,
        /// Сериализованные параметры операции.
        payload: Vec<u8>,
        /// Ожидаемая ревизия конфигурации адаптера.
        expected_revision: u64,
        /// Ключ для безопасного повтора изменяющей операции.
        idempotency_key: String,
        /// Идентификатор для корреляции с внешним запросом.
        correlation_id: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Возвращает снимок состояния задачи в проекте.
    GetTaskSnapshot {
        /// Идентификатор проекта.
        project_id: String,
        /// Идентификатор задачи, состояние которой нужно прочитать.
        task_id: String,
        /// Канал снимка или ошибки чтения.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Восстанавливает задачу из ранее созданного снимка.
    RestoreTaskSnapshot {
        /// Идентификатор проекта-владельца задачи.
        project_id: String,
        /// Идентификатор восстанавливаемой задачи.
        task_id: String,
        /// Идентификатор снимка, выбранного для восстановления.
        snapshot_id: String,
        /// Канал результата восстановления.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Читает build policy проекта.
    GetBuildPolicy {
        /// Идентификатор проекта, policy которого читается.
        project_id: String,
        /// Канал policy или ошибки чтения.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Сохраняет build policy при совпадении ожидаемой версии.
    SaveBuildPolicy {
        /// Идентификатор проекта-владельца policy.
        project_id: String,
        /// JSON-документ новой policy; обработчик проверяет его схему.
        policy_json: Vec<u8>,
        /// Версия policy, прочитанная до редактирования.
        expected_version: i64,
        /// Канал результата сохранения или конфликта версии.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Применяет только сборку, ранее одобренную через build policy.
    ApplyApprovedBuild {
        /// Идентификатор проекта.
        project_id: String,
        /// Идентификатор запуска сборки.
        run_id: String,
        /// Идентификатор задачи, в рамках которой одобрена сборка.
        task_id: String,
        /// Подписанное/проверяемое описание одобренной сборки.
        approved_build_json: Vec<u8>,
        /// Канал результата применения и проверки approval.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Проверяет предложение сборки и подготавливает его к отдельному approval.
    PrepareBuild {
        /// Идентификатор проекта, для которого подготовлена сборка.
        project_id: String,
        /// JSON-описание предлагаемой сборки.
        proposal_json: Vec<u8>,
        /// Канал подготовленного результата или ошибки проверки.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Bounded, read-only Core Doctor diagnostic. `project_id` is optional;
    /// when set, the permissions probe is grounded in that project's real
    /// workspace path. `protocol_major`/`expected_protocol_major` and
    /// `provider`/`approval_required` are supplied by the IPC layer, which
    /// is where that state actually lives.
    RunDoctor {
        /// Проект для проверки workspace permissions; `None` пропускает её.
        project_id: String,
        /// Major-версия протокола, сообщённая IPC transport.
        protocol_major: Option<u32>,
        /// Major-версия протокола, ожидаемая Core.
        expected_protocol_major: u32,
        /// Безопасная сводка состояния провайдера без учётных данных.
        provider: crate::doctor::ProviderProbe,
        /// Требует ли активная операция явного approval.
        approval_required: bool,
        /// Число зарегистрированных инструментов.
        registered_tools: u32,
        /// Ожидаемое число инструментов в актуальном registry.
        expected_tools: u32,
        /// Названия недоступных инструментов без чувствительных параметров.
        unavailable_tools: Vec<String>,
        /// Уровень детализации ограниченного отчёта.
        detail_level: crate::doctor::DetailLevel,
        /// Канал JSON-результата диагностики.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Bounded, read-only support snapshot. It never persists diagnostics or
    /// reads raw prompts, workspace files, credentials, or tool payloads.
    CreateDiagnosticsSnapshot {
        /// Проект для определения ограниченного workspace-контекста.
        project_id: String,
        /// Идентификатор разговора, попадающего в snapshot.
        conversation_id: String,
        /// Идентификатор запуска, попадающего в snapshot.
        run_id: String,
        /// Максимальное число событий в snapshot.
        max_event_count: u32,
        /// Максимальный суммарный размер журналов в байтах.
        max_log_bytes: u32,
        /// Major-версия протокола, сообщённая IPC transport.
        protocol_major: Option<u32>,
        /// Major-версия протокола, ожидаемая Core.
        expected_protocol_major: u32,
        /// Безопасная сводка состояния провайдера.
        provider: crate::doctor::ProviderProbe,
        /// Указывает, ожидает ли операция approval.
        approval_required: bool,
        /// Текущее число зарегистрированных инструментов.
        registered_tools: u32,
        /// Ожидаемое число инструментов.
        expected_tools: u32,
        /// Имена недоступных инструментов.
        unavailable_tools: Vec<String>,
        /// Канал bounded/redacted snapshot или ошибки его формирования.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Exports the local `logs/core.jsonl` (and `supervisor.jsonl`, when
    /// present) plus recent `run_tool_metrics` aggregates to a caller-chosen
    /// destination path, redacted the same way hook payloads are. Never
    /// touches eval fixtures or feedback storage.
    ExportDoctorLogs {
        /// Файловый путь назначения для ограниченного и очищенного экспорта.
        destination_path: String,
        /// Канал результата экспорта или ошибки доступа к файлу.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Создаёт согласованную резервную копию базы данных.
    CreateDatabaseBackup {
        /// Идентификатор длительной операции backup.
        operation_id: String,
        /// Путь, куда записывается резервная копия.
        destination_path: String,
        /// Канал прогресса; закрывается после завершения или ошибки.
        progress: mpsc::UnboundedSender<BackupProgress>,
        /// Канал итогового результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Проверяет резервную копию и подготавливает отдельную операцию restore.
    PrepareDatabaseRestore {
        /// Идентификатор операции проверки.
        operation_id: String,
        /// Путь резервной копии, которую нужно проверить.
        backup_path: String,
        /// Канал результата предварительной проверки.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Восстанавливает базу данных из копии после одобрения.
    RestoreDatabase {
        /// Идентификатор длительной операции восстановления.
        operation_id: String,
        /// Путь проверенной резервной копии.
        backup_path: String,
        /// Идентификатор ранее полученного approval для destructive restore.
        approval_id: String,
        /// Канал прогресса восстановления.
        progress: mpsc::UnboundedSender<BackupProgress>,
        /// Канал итогового результата восстановления.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Запрашивает отмену активной backup/restore операции.
    CancelDatabaseOperation {
        /// Идентификатор операции, подлежащей отмене.
        operation_id: String,
        /// Канал подтверждения запроса отмены.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Captures one bounded, redacted piece of offline research evidence and
    /// persists it against the real `research_evidence` table, tied to
    /// `work_item_id` via `provenance_link`. Redaction and validation happen
    /// in `research::ResearchEvidence::capture` before anything is stored.
    SaveResearchEvidence {
        /// Work item, с которым связывается свидетельство.
        work_item_id: String,
        /// Категория первоисточника.
        source_kind: String,
        /// Ссылка или внешний идентификатор первоисточника.
        source_ref: String,
        /// Заголовок источника.
        title: String,
        /// Имя издателя или организации-источника.
        publisher: String,
        /// Тип содержимого извлечённого свидетельства.
        content_type: String,
        /// Небольшой исходный фрагмент, который проходит redaction до записи.
        raw_excerpt: String,
        /// Время получения в Unix milliseconds.
        retrieved_at_ms: u64,
        /// Срок хранения evidence в миллисекундах.
        ttl_ms: u64,
        /// Канал результата сохранения.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists previously saved research evidence for a work item.
    ListResearchEvidence {
        /// Work item, для которого запрашиваются сохранённые свидетельства.
        work_item_id: String,
        /// Канал ограниченного списка evidence.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Performs a real, policy-gated, SSRF-protected HTTP GET against `url`,
    /// driving `research_fetch::run_research_fetch` through the real
    /// `research_pipeline` state machine, then persists the resulting
    /// `ResearchEvidence` the same way `SaveResearchEvidence` does. `title`
    /// is caller-supplied; content-type/publisher are derived from the
    /// response and URL. No search-engine integration and no LLM-based
    /// summarization happen here (see `research_fetch` module docs).
    RunResearchFetch {
        /// Work item, с которым сохраняется результат fetch.
        work_item_id: String,
        /// URL, который Core проверит по network/SSRF policy.
        url: String,
        /// Название свидетельства, заданное вызывающим кодом.
        title: String,
        /// Список разрешённых доменов для этого fetch.
        allowed_domains: Vec<String>,
        /// Верхняя граница загружаемого тела в байтах.
        max_bytes: u64,
        /// Максимальная длительность fetch в миллисекундах.
        max_latency_ms: u64,
        /// Верхняя граница стоимости операции в микродолларах.
        max_cost_micros: u64,
        /// Срок хранения полученного evidence в миллисекундах.
        ttl_ms: u64,
        /// Канал metadata-проекции evidence или ошибки policy/fetch.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Runs the bounded research adapter and returns only a metadata
    /// projection. The caller supplies already-selected result URLs; every
    /// URL still passes the normal network and fetch policy.
    RunGroundedResearchSession {
        /// Сериализованные результаты/URL для bounded grounded session.
        payload: Vec<u8>,
        /// Канал metadata-проекции или ошибки выполнения.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Creates one bounded Memory v1 record. `memory_domain::MemoryDomain`
    /// runs validation, TTL expansion and content redaction server-side
    /// (its in-memory storage is not used: the real `memory_entries` table,
    /// via `memory_store`, is the sole source of truth); `id` and
    /// `created_at_ms` are computed here, never trusted from the caller.
    CreateMemory {
        /// Вид области памяти, например project или task.
        scope_kind: String,
        /// Идентификатор проекта; Core вычисляет нормализованный scope id.
        project_id: String,
        /// Дополнительный идентификатор точной области, если он используется.
        secondary_id: String,
        /// Краткий заголовок записи памяти.
        title: String,
        /// Содержимое записи; серверная policy проверяет и редактирует его.
        content: String,
        /// Тип provenance, объясняющий происхождение сведений.
        provenance_kind: String,
        /// Идентификатор записи или события-первоисточника.
        provenance_id: String,
        /// Locator первоисточника без копирования его содержимого.
        provenance_locator: String,
        /// Классификация приватности, проверяемая Memory policy.
        privacy: String,
        /// Срок жизни записи в миллисекундах.
        ttl_ms: u64,
        /// Канал созданной записи или ошибки валидации.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists non-forgotten Memory v1 records for one exact scope.
    ListMemory {
        /// Вид области поиска.
        scope_kind: String,
        /// Идентификатор проекта точной области.
        project_id: String,
        /// Вторичный идентификатор области, например задачи.
        secondary_id: String,
        /// Включать ли архивные записи в выдачу.
        include_archived: bool,
        /// Максимальное число возвращаемых записей.
        limit: u32,
        /// Канал metadata-списка без тел записей.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lexical, deterministic search over Memory v1 records for one exact
    /// scope.
    SearchMemory {
        /// Вид области поиска.
        scope_kind: String,
        /// Идентификатор проекта точной области.
        project_id: String,
        /// Вторичный идентификатор области, например задачи.
        secondary_id: String,
        /// Поисковая фраза для детерминированного lexical search.
        query: String,
        /// Максимальное число совпадений.
        limit: u32,
        /// Канал metadata-результатов без содержимого записей.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Archives a memory record. Per the Memory v1 plan, this requires an
    /// out-of-band approval token (`approval_id`), validated the same way
    /// `memory_api::Approval` validates it: mirrors the `ApplyApprovedBuild`
    /// trust model, where the client presents proof that the operation was
    /// already approved before this command is sent.
    ArchiveMemory {
        /// Идентификатор записи, которую нужно архивировать.
        id: String,
        /// Approval-токен, заранее проверенный по установленному trust model.
        approval_id: String,
        /// Канал результата архивирования.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Permanently erases a memory record's title/content. Also requires an
    /// out-of-band approval token; see `ArchiveMemory`. Writes a tombstone
    /// carrying only metadata and a digest.
    ForgetMemory {
        /// Идентификатор записи, содержимое которой стирается.
        id: String,
        /// Approval-токен, необходимый для необратимого удаления содержимого.
        approval_id: String,
        /// Канал результата операции и записи tombstone.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Creates/inspects a Core-owned MemoryView and records bounded adaptive
    /// recall decisions. The payload contains no memory bodies or credentials.
    MemoryViewsAndAdaptiveRecall {
        /// Вид операции над view или адаптивным recall.
        operation: String,
        /// Идентификатор представления памяти.
        view_id: String,
        /// Сериализованный bounded-контекст без тел записей и credentials.
        payload: Vec<u8>,
        /// Ожидаемая версия view.
        expected_version: u64,
        /// Ключ для безопасного повтора изменяющей операции.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Reads and updates versioned model-edit protocol registry entries.
    ModelEditProtocolRegistry {
        /// Вид операции над реестром протоколов редактирования.
        operation: String,
        /// Идентификатор протокола редактирования.
        protocol_id: String,
        /// Сериализованные данные протокола или команды.
        payload: Vec<u8>,
        /// Ожидаемая версия записи протокола.
        expected_version: u64,
        /// Ключ, предотвращающий повторное применение изменения.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Reads and updates remote conversation channel state.
    RemoteConversationChannels {
        /// Вид операции над удалённым каналом.
        operation: String,
        /// Идентификатор соединения канала.
        connection_id: String,
        /// Сериализованные данные канала или операции.
        payload: Vec<u8>,
        /// Ожидаемая версия записи канала.
        expected_version: u64,
        /// Ключ для безопасного повтора операции.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Plans versioned prompt-cache configurations.
    PromptCachePlanner {
        /// Вид операции планировщика prompt cache.
        operation: String,
        /// Идентификатор плана кэширования.
        plan_id: String,
        /// Сериализованные входные данные плана.
        payload: Vec<u8>,
        /// Ожидаемая версия плана.
        expected_version: u64,
        /// Ключ для распознавания повторного изменения.
        idempotency_key: String,
        /// Канал результата планирования.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Reads and updates declarative runtime component records.
    DeclarativeRuntimeComponents {
        /// Вид операции над компонентом runtime.
        operation: String,
        /// Идентификатор декларативного компонента.
        component_id: String,
        /// Сериализованные описание компонента или запрос.
        payload: Vec<u8>,
        /// Ожидаемая версия записи компонента.
        expected_version: u64,
        /// Ключ, предотвращающий повторное применение изменения.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Manages guided calibration session state.
    GuidedCalibrationSessions {
        /// Вид операции калибровочной сессии.
        operation: String,
        /// Идентификатор сессии калибровки.
        session_id: String,
        /// Сериализованные данные сессии или команды.
        payload: Vec<u8>,
        /// Ожидаемая версия сессии.
        expected_version: u64,
        /// Ключ, предотвращающий повторное изменение сессии.
        idempotency_key: String,
        /// Канал результата операции.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Runs and records extension conformance operations.
    ExtensionConformanceKit {
        /// Вид операции проверки соответствия.
        operation: String,
        /// Идентификатор проверяемого субъекта/расширения.
        subject_id: String,
        /// Сериализованные данные субъекта или conformance report.
        payload: Vec<u8>,
        /// Ожидаемая версия проверки.
        expected_version: u64,
        /// Ключ для безопасного повтора проверки/изменения.
        idempotency_key: String,
        /// Канал результата проверки соответствия.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Manages persisted agent organization and ownership records.
    PersistentAgentOrganizationRegistry {
        /// Registry operation to perform.
        operation: String,
        /// Agent whose organization record is addressed.
        agent_id: String,
        /// Scope that owns the organization record.
        owner_scope: String,
        /// Actor initiating the change; validated by the registry policy.
        actor: String,
        /// Serialized operation-specific record or parameters.
        payload: Vec<u8>,
        /// Expected record revision for optimistic concurrency control.
        expected_revision: u64,
        /// Key that makes retries of a mutation idempotent.
        idempotency_key: String,
        /// Channel for the operation result or policy failure.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Manages a scoped execution environment profile.
    ExecutionEnvironmentProfile {
        /// Profile operation to perform.
        operation: String,
        /// Identifier of the execution environment profile.
        profile_id: String,
        /// Scope that owns the profile.
        owner_scope: String,
        /// Serialized profile data or operation parameters.
        payload: Vec<u8>,
        /// Expected profile revision for conditional updates.
        expected_revision: u64,
        /// Key that makes retries of a mutation idempotent.
        idempotency_key: String,
        /// Channel for the operation result.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Manages a versioned namespace for context records.
    ContextNamespace {
        /// Namespace operation to perform.
        operation: String,
        /// Identifier of the namespace.
        namespace_id: String,
        /// Serialized namespace data or operation parameters.
        payload: Vec<u8>,
        /// Expected namespace revision for conditional updates.
        expected_revision: u64,
        /// Key that makes retries of a mutation idempotent.
        idempotency_key: String,
        /// Channel for the operation result.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Manages durable background execution state for a run.
    DurableBackgroundExecution {
        /// Operation to perform on the background run.
        operation: String,
        /// Identifier of the background run.
        run_id: String,
        /// Scope that owns the run.
        owner_scope: String,
        /// Serialized run state or operation parameters.
        payload: Vec<u8>,
        /// Expected run revision for conditional updates.
        expected_revision: u64,
        /// Key that prevents retries from applying the same mutation twice.
        idempotency_key: String,
        /// Channel for the operation result.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Reads one memory record including its body. `sensitive`, forgotten and
    /// empty records come back redacted: `ListMemory` never carries a body,
    /// and this is the only path that can.
    GetMemory {
        /// Идентификатор записи. Чувствительное содержимое возвращается redacted.
        id: String,
        /// Канал результата чтения записи.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists the pending-confirmation queue plus per-state counters for one
    /// exact scope. Metadata only.
    ListMemoryPending {
        /// Вид точной области pending-записей.
        scope_kind: String,
        /// Проект, которому принадлежит область.
        project_id: String,
        /// Дополнительный идентификатор области.
        secondary_id: String,
        /// Максимальное число результатов.
        limit: u32,
        /// When non-empty, Core derives the workspace scope id itself, which
        /// is the scope memory extraction writes under.
        workspace_path: String,
        /// Канал metadata очереди и счётчиков статусов.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Deterministic conflicts between pending records and the currently
    /// active memory of the same `kind + canonical_subject + scope`. Reading
    /// conflicts never changes any record: an unresolved conflict leaves the
    /// old entry active and the new one pending.
    GetMemoryConflicts {
        /// Вид точной области проверки конфликтов.
        scope_kind: String,
        /// Идентификатор проекта области.
        project_id: String,
        /// Дополнительный идентификатор области, если нужен.
        secondary_id: String,
        /// Максимальное число конфликтов в ответе.
        limit: u32,
        /// Workspace path для вычисления фактического project scope.
        workspace_path: String,
        /// Канал чтения конфликтов без изменения активных записей.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Confirms one or more pending records. Requires an out-of-band approval
    /// token (`approval_id`) and an `idempotency_key`; repeating the same
    /// request is safe and reports the actual current state of each id.
    ConfirmMemory {
        /// Идентификаторы pending-записей, подтверждаемых одной операцией.
        ids: Vec<String>,
        /// Внешний approval, проверяемый перед подтверждением.
        approval_id: String,
        /// Ключ идемпотентности для безопасного повторения всего набора.
        idempotency_key: String,
        /// Канал актуального состояния каждой обработанной записи.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Rejects one or more pending records. Same trust model as
    /// `ConfirmMemory`; a rejected record is terminal and never reopens.
    RejectMemory {
        /// Идентификаторы pending-записей, которые отклоняются.
        ids: Vec<String>,
        /// Внешний approval, проверяемый перед отклонением.
        approval_id: String,
        /// Ключ идемпотентности для безопасного повтора операции.
        idempotency_key: String,
        /// Канал результата отклонения.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Edits a pending candidate before confirmation, or keeps it only for the
    /// current session. Neither action confirms anything by itself.
    ReviseMemoryCandidate {
        /// Идентификатор pending-кандидата.
        id: String,
        /// Новое текстовое утверждение для кандидата.
        statement: String,
        /// Ограничить новую версию текущей сессией без подтверждения.
        session_only: bool,
        /// Сессия, в которой действует session-only версия.
        session_id: String,
        /// Внешний approval для редактирования кандидата.
        approval_id: String,
        /// Ключ безопасного повтора редактирования.
        idempotency_key: String,
        /// Канал результата изменения кандидата.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Resolves a conflict by an explicit user choice: `old_id` is superseded
    /// by `new_id` with a mandatory reason. Supersede happens only here, never
    /// automatically.
    SupersedeMemory {
        /// Идентификатор активной старой записи, которую заменяют.
        old_id: String,
        /// Идентификатор новой записи, выбранной вместо старой.
        new_id: String,
        /// Обязательное объяснение явного решения о замене.
        reason: String,
        /// Внешний approval для операции supersede.
        approval_id: String,
        /// Ключ для безопасного повтора решения.
        idempotency_key: String,
        /// Канал результата разрешения конфликта.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Installs (or, when a manifest of the same name already exists,
    /// updates) one bounded capability manifest into the local catalog.
    /// `manifest_json` is validated via
    /// `capability_registry::CapabilityManifest`'s own bounds plus
    /// `validate_registry`/`validate_update` against the manifests already
    /// persisted, before anything is written. `local_archive` carries only
    /// an audit path. `https_archive` treats `source_path` as an HTTPS URL,
    /// downloads it through the shared SSRF guard, and requires the trusted
    /// out-of-band SHA-256 in `expected_content_hash` to match before any
    /// catalog write.
    InstallCapability {
        /// JSON-манифест; Core проверяет границы и конфликты перед записью.
        manifest_json: String,
        /// Тип источника установки (`local_archive` или проверяемый HTTPS).
        install_source: String,
        /// Локальный путь архива либо HTTPS URL согласно `install_source`.
        source_path: String,
        /// Ожидаемый SHA-256 скачанного содержимого для HTTPS-источника.
        expected_content_hash: String,
        /// Канал результата установки после проверки манифеста и содержимого.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists installed capability manifests, newest-first.
    ListCapabilities {
        /// Максимальное число manifests в ответе.
        limit: u32,
        /// Канал списка установленных манифестов.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Deterministic intent/tool/domain match against the installed
    /// catalog, via `capability_registry::match_capabilities`.
    MatchCapabilities {
        /// Описание пользовательского намерения, используемое matcher-ом.
        intent: String,
        /// Инструменты, обязательные для подходящего capability.
        required_tools: Vec<String>,
        /// Домены, обязательные для подходящего capability.
        required_domains: Vec<String>,
        /// Максимальный допустимый риск запроса.
        requested_risk: String,
        /// Канал детерминированного списка совпадений.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Removes one installed capability manifest by id (manifest name).
    RemoveCapability {
        /// Имя manifest, используемое как идентификатор реестровой записи.
        id: String,
        /// Канал результата удаления.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Runtime/UI wiring for capability-registry selection
    /// (`capability_selection::select_for_task`/`reconcile_with_pin`): runs
    /// the deterministic matcher for the query, reconciles against any
    /// selection already persisted for `task_id`, persists the reconciled
    /// state, and returns it.
    GetCapabilitySelection {
        /// Идентификатор задачи, для которой хранится выбор capability.
        task_id: String,
        /// Текст задачи для детерминированного matcher-а.
        intent: String,
        /// Требуемые инструменты.
        required_tools: Vec<String>,
        /// Требуемые домены.
        required_domains: Vec<String>,
        /// Верхняя граница допустимого риска.
        requested_risk: String,
        /// Канал согласованного и сохранённого выбора.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Pins the selection persisted for `task_id`
    /// (`capability_selection::pin`) so future `GetCapabilitySelection`
    /// calls cannot silently swap it. Fails if no selection is persisted
    /// yet for `task_id`.
    PinCapabilitySelection {
        /// Идентификатор задачи с уже сохранённым выбором.
        task_id: String,
        /// Канал результата фиксации выбора.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Explicitly switches the selection persisted for `task_id` to
    /// `manifest_name` (`capability_selection::replace`), re-deriving
    /// permissions/reasons against the same query.
    ReplaceCapabilitySelection {
        /// Идентификатор задачи, чья текущая capability selection заменяется.
        task_id: String,
        /// Имя выбранного вместо текущего manifest.
        manifest_name: String,
        /// Текст задачи, относительно которого пересчитываются права.
        intent: String,
        /// Требуемые инструменты.
        required_tools: Vec<String>,
        /// Требуемые домены.
        required_domains: Vec<String>,
        /// Верхняя граница допустимого риска.
        requested_risk: String,
        /// Канал нового сохранённого выбора или ошибки проверки.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Validates and persists one bounded, redacted task handoff between
    /// child roles (`child_roles::HandoffEnvelope::new`). This only records
    /// the handoff; it does not deliver or act on it for any real child
    /// agent -- runtime wiring remains a later, dedicated task per
    /// `child_roles.rs`'s own scope note.
    RequestChildHandoff {
        /// Уникальный идентификатор передачи.
        handoff_id: String,
        /// Идентификатор родительской задачи.
        task_id: String,
        /// Тип передаваемого пакета.
        kind: String,
        /// Роль исходного child.
        from_role: String,
        /// Имя исходного child.
        from_name: String,
        /// Роль получателя.
        to_role: String,
        /// Имя получателя.
        to_name: String,
        /// Ограниченная цель передачи.
        purpose: String,
        /// Поля пакета; Core валидирует и редактирует их до сохранения.
        payload: std::collections::HashMap<String, String>,
        /// Монотонный номер передачи в последовательности задачи.
        sequence: u64,
        /// Канал сохранённой записи или ошибки проверки.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists persisted child handoffs for a task, in sequence order.
    ListChildHandoffs {
        /// Задача, для которой читается история передач.
        task_id: String,
        /// Максимальное число записей в ответе.
        limit: u32,
        /// Канал передач в порядке sequence.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Validates (`child_runtime::ChildTaskRequest::validate`) and persists
    /// one bounded, read-only child task request. Rejects any request with
    /// a non-read-only `requested_capabilities` entry, any nested child
    /// (`parent_is_child = true`), or oversized context/output -- the same
    /// pure contract used by the unit tests, enforced end-to-end here. Core
    /// does not act on an accepted request: it is stored as a durable
    /// record of an approved read-only child task descriptor for whatever
    /// later spawns it (out of scope for this task).
    SubmitChildRequest {
        /// Идентификатор дочерней задачи.
        child_task_id: String,
        /// Идентификатор родительской задачи.
        parent_task_id: String,
        /// Роль child в рамках запроса.
        role: String,
        /// Тип read-only работы.
        kind: String,
        /// Ограниченный контекст, передаваемый child.
        reduced_context: Vec<String>,
        /// Верхняя граница результата child в байтах.
        max_output_bytes: u32,
        /// Запрашиваемые права; каждая возможность должна быть read-only.
        requested_capabilities: Vec<String>,
        /// Признак вложенного child; вложенные запросы отклоняются.
        parent_is_child: bool,
        /// Канал сохранённого задания либо ошибки валидации.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Validates (`child_runtime::accept_report`, against the matching
    /// stored `SubmitChildRequest`) and persists one child report. Rejects
    /// a task-id mismatch, secret-like content, duplicate sources, or a
    /// missing/invalid matching request.
    SubmitChildReport {
        /// Идентификатор child task, совпадающий с сохранённым запросом.
        child_task_id: String,
        /// Итоговый статус дочерней задачи.
        status: String,
        /// Краткое текстовое резюме результата.
        summary: String,
        /// Выводы с ограничением размера и секретов.
        findings: Vec<String>,
        /// Идентификаторы источников, использованных child.
        sources: Vec<String>,
        /// Самооценка уверенности в процентах.
        confidence_percent: u32,
        /// Канал сохранённого отчёта или причины отказа.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Persists one bounded, redacted feedback record (useful/not-useful,
    /// optional correction, optional rejection reason) against the real
    /// `feedback_entries` table. `run_id` must correlate to an existing
    /// `runs.id`; `subject_ref` is an existing tool-call/effect/approval id
    /// when the feedback is about a specific result, not a newly minted
    /// correlation id. Local-only: this command never sends data anywhere,
    /// see `evohime_local_storage::feedback_store::external_telemetry_allowed`.
    SubmitFeedback {
        /// Существующий запуск, к которому относится отзыв.
        run_id: String,
        /// Задача запуска, если отзыв относится к конкретной задаче.
        task_id: Option<String>,
        /// Существующий tool-call/effect/approval, если отзыв касается результата.
        subject_ref: Option<String>,
        /// Сигнал полезности результата.
        signal: String,
        /// Необязательное исправление или уточнение от пользователя.
        correction: Option<String>,
        /// Необязательная причина отклонения.
        rejection_reason: Option<String>,
        /// Необязательный исход, которым завершилась работа.
        outcome: Option<String>,
        /// Канал результата локального сохранения.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists feedback for one run (newest first) plus the local aggregation
    /// (signal counts, top rejection reasons/outcomes by frequency).
    ListFeedback {
        /// Запуск, отзывы по которому нужно получить.
        run_id: String,
        /// Максимальное число записей в ответе.
        limit: u32,
        /// Канал отсортированных записей и их локальной агрегации.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Incremental bounded workspace indexing. The scanner and SQLite
    /// generation are owned by Core; UI supplies only the selected root.
    IndexWorkspace {
        /// Выбранный корень workspace, который Core просканирует.
        workspace_path: String,
        /// Разрешено ли индексатору строить embedding-представления.
        enable_embeddings: bool,
        /// Канал статуса индексирования или ошибки.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Controlled full rebuild. The previous published generation remains
    /// visible until the new one passes consistency checks and publication.
    RebuildIndex {
        /// Workspace, для которого строится новая генерация индекса.
        workspace_path: String,
        /// Разрешено ли включать embedding-представления.
        enable_embeddings: bool,
        /// Канал результата после consistency check и публикации генерации.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Requests cancellation of indexing for a workspace.
    CancelWorkspaceIndex {
        /// Корень workspace активной операции индексирования.
        workspace_path: String,
        /// Канал результата запроса отмены.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Bounded lexical/hybrid retrieval with planner/checker diagnostics and
    /// validated source metadata.
    SearchWorkspaceKnowledge {
        /// Workspace, индекс которого используется для поиска.
        workspace_path: String,
        /// Поисковый запрос.
        query: String,
        /// Необязательное ограничение результатов по относительному пути.
        path_filter: Option<String>,
        /// Необязательное ограничение результатов по языку.
        language_filter: Option<String>,
        /// Включает гибридное lexical/embedding ранжирование.
        hybrid: bool,
        /// Канал bounded результатов с валидированными метаданными источников.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Read-only bounded status projection for the selected workspace.
    GetIndexStatus {
        /// Workspace, для которого строится status projection.
        workspace_path: String,
        /// Канал read-only статуса индекса.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// План 01.5: bounded projection состава контекста последних model call.
    GetContextLedger {
        /// Задача, чьи model-call ledger entries запрашиваются.
        task_id: String,
        /// Максимальное число последних записей.
        limit: u32,
        /// Канал ограниченной проекции ledger.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Bounded чтение scratchpad задачи с фильтром по категории и статусу.
    ListTaskScratchpad {
        /// Задача, для которой читаются scratchpad-записи.
        task_id: String,
        /// Необязательный фильтр по категории записи.
        category: Option<String>,
        /// Необязательный фильтр по статусу записи.
        status: Option<String>,
        /// Максимальное число записей.
        limit: u32,
        /// Канал bounded списка scratchpad.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Очистка task-scoped scratchpad. Mutation с записью аудита.
    ClearTaskScratchpad {
        /// Задача, для которой выполняется очищаемая операция.
        task_id: String,
        /// Канал результата очистки и аудита.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Принудительное сжатие текущей сборки контекста задачи.
    SummarizeContextNow {
        /// Задача, для которой принудительно запускается сокращение контекста.
        task_id: String,
        /// Канал результата планирования или отказа по бюджету.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// `pin/unpin item`: выставляет флаг `pinned` из 01.1.
    PinContextItem {
        /// Задача-владелец элемента контекста.
        task_id: String,
        /// Идентификатор элемента, который нужно закрепить или снять с закрепления.
        item_id: String,
        /// Новое значение признака закрепления.
        pinned: bool,
        /// Канал результата изменения выбора.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Чтение полного содержимого артефакта с повторной policy-проверкой.
    ReadContextArtifact {
        /// Задача-владелец ссылки на artifact.
        task_id: String,
        /// Locator артефакта, который повторно проверяется через policy.
        locator: String,
        /// Канал содержимого или отказа повторной проверки.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Retains a validated child record under its parent task.
    RetainChild {
        /// Проверенная версия child record для долговременного хранения.
        child: crate::retained_child::RetainedChildV1,
        /// Текущее время в Unix milliseconds для проверки TTL.
        now_ms: u64,
        /// Канал сохранённой записи или ошибки валидации.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Loads one retained child record with expiry and redaction checks.
    GetRetainedChild {
        /// Идентификатор родительской задачи.
        parent_id: String,
        /// Идентификатор сохранённого child.
        child_id: String,
        /// Текущее время для проверки срока хранения.
        now_ms: u64,
        /// Канал bounded child record или результата redaction.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Sends a validated follow-up request to a retained child.
    SendChildFollowUp {
        /// Проверенный запрос follow-up к сохранённому child.
        request: crate::retained_child::ChildFollowUpRequestV1,
        /// Текущее время в Unix milliseconds.
        now_ms: u64,
        /// Указывает, занят ли child другой операцией.
        busy: bool,
        /// Канал результата отправки или ошибки busy/policy.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists retained children for a parent task within the supplied limit.
    ListRetainedChildren {
        /// Идентификатор родительской задачи-владельца записей.
        parent_id: String,
        /// Текущее время для исключения истёкших записей.
        now_ms: u64,
        /// Максимальное число результатов.
        limit: u32,
        /// Канал bounded списка сохранённых child.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Deletes a retained child using optimistic registry-version checking.
    DeleteRetainedChild {
        /// Идентификатор родительской задачи.
        parent_id: String,
        /// Идентификатор child, который удаляется из реестра.
        child_id: String,
        /// Версия реестра, которую прочитал вызывающий код.
        expected_registry_version: u64,
        /// Канал результата удаления или конфликта версии.
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
}

/// Bounded events produced by the Core and projected to desktop clients.
/// Sensitive prompt, tool, and context payloads are controlled by each event's
/// contract; the IPC adapter serializes these values for transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum CoreEvent {
    /// Sends the final prompt assembled for one model invocation.
    ModelContext {
        /// Task that owns the model invocation.
        task_id: String,
        /// Workspace used to build the prompt.
        workspace_path: String,
        /// Selected model identifier.
        model: String,
        /// System instructions sent to the model.
        system_prompt: String,
        /// User/context messages sent to the model.
        user_prompt: String,
        /// Tool names made available to the model.
        tools: Vec<String>,
        /// Estimated prompt size in tokens.
        estimated_tokens: usize,
        /// Effective context-window limit in tokens.
        context_limit_tokens: usize,
        /// План 01.5: additive bounded projection состава контекста. Старые
        /// клиенты игнорируют неизвестное поле, поэтому major bump не нужен.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<Box<crate::context_budget::ModelContextProjection>>,
    },
    /// Terminal Core-owned routing decision. Intermediate attempts stay in
    /// diagnostics; the renderer receives this bounded projection only.
    RoutingTrace {
        /// Task that produced the route decision.
        task_id: String,
        /// Bounded routing trace, including the terminal decision and diagnostics.
        trace: evohime_model_gateway::RoutingTrace,
    },
    /// Non-terminal request to approve a policy-controlled reroute.
    PendingRoutingApproval {
        /// Task waiting for the user's routing decision.
        task_id: String,
        /// Identifier used to resolve the pending decision.
        trace_id: String,
        /// Run associated with the proposed reroute.
        run_id: String,
        /// Proposed route identifier.
        route_id: String,
        /// Expiration time of the approval in Unix milliseconds.
        expires_at_ms: u64,
    },
    /// Announces that Core accepted and began a task.
    TaskStarted {
        /// Identifier of the started task.
        task_id: String,
        /// User request associated with the task.
        prompt: String,
    },
    /// Streams a visible assistant text fragment for a task.
    AssistantDelta {
        /// Identifier of the task producing the fragment.
        task_id: String,
        /// Text fragment appended to the assistant response.
        content: String,
    },
    /// Announces that Core started a tool invocation.
    ToolStarted {
        /// Identifier of the task that invoked the tool.
        task_id: String,
        /// Registered tool name.
        tool_name: String,
    },
    /// Reports the bounded output of a tool invocation.
    ToolOutput {
        /// Identifier of the task that invoked the tool.
        task_id: String,
        /// Registered tool name.
        tool_name: String,
        /// Redacted output presented to the conversation.
        output: String,
    },
    /// Requests user approval before Core executes a protected tool action.
    ApprovalRequired {
        /// Identifier of the task awaiting approval.
        task_id: String,
        /// Approval identifier used to approve or deny the action.
        approval_id: String,
        /// Tool that requested the protected operation.
        tool_name: String,
        /// Permission required by the operation.
        permission: String,
        /// Scope within which the operation would be applied.
        scope: String,
        /// Bounded preview shown to the user before approval.
        preview: evohime_permissions::ApprovalPreview,
    },
    /// Reports successful completion of a task.
    TaskCompleted {
        /// Identifier of the completed task.
        task_id: String,
        /// Final assistant response.
        final_message: String,
    },
    /// Reports that a task ended with an error.
    TaskFailed {
        /// Identifier of the failed task.
        task_id: String,
        /// User-safe error summary.
        error: String,
    },
    /// Reports that a task was stopped before normal completion.
    TaskStopped {
        /// Identifier of the stopped task.
        task_id: String,
    },
    /// Reports an event append failure without replacing the original operation error.
    EventPersistenceFailed {
        /// Persistence boundary that failed.
        source: String,
        /// Sanitized description of the persistence error.
        error: String,
    },
    /// Publishes bounded progress for a long-running code review.
    ReviewProgress {
        /// Identifier of the review operation.
        review_id: String,
        /// Current review stage.
        stage: String,
        /// Status within the current stage.
        status: String,
        /// Model used for this stage, when one is selected.
        model: Option<String>,
        /// Number of completed review units.
        completed: usize,
        /// Total number of review units.
        total: usize,
    },
    /// Publishes progress for a document or plan revision.
    RevisionProgress {
        /// Identifier of the revision operation.
        revision_id: String,
        /// Current revision state.
        status: String,
        /// Model used to produce the revision.
        model: String,
    },
    /// Publishes progress for a database backup or restore operation.
    StorageProgress {
        /// Identifier of the storage operation.
        operation_id: String,
        /// Current bounded progress counters and status.
        progress: BackupProgress,
    },
    /// Publishes progress while indexing a workspace.
    WorkspaceIndexProgress {
        /// Workspace being indexed.
        workspace_path: String,
        /// Current bounded indexing counters and phase.
        progress: crate::workspace_rag::IndexProgress,
    },
    /// Publishes progress while retrieving workspace knowledge.
    WorkspaceRetrievalProgress {
        /// Workspace queried by the retrieval operation.
        workspace_path: String,
        /// Current bounded retrieval counters and phase.
        progress: crate::workspace_rag::RetrievalProgress,
    },
    /// Bounded Core-owned child workflow projection for UI/timeline consumers.
    ChildWorkflowProjection {
        /// Parent task owning the child workflow.
        task_id: String,
        /// Bounded event suitable for UI/timeline projection.
        projection: crate::child_workflow::ChildProjection,
    },
    /// Bounded projection события durable workflow run (план 06.2).
    ///
    /// Полезная нагрузка ограничена идентификаторами, состояниями и кодами:
    /// ни prompt, ни сырой вывод child, ни содержимое контекста в неё не
    /// попадают.
    WorkflowProgress {
        /// Identifier of the durable workflow run.
        run_id: String,
        /// Projection contains only bounded IDs, states, and error codes.
        projection: Box<crate::workflow_runtime::WorkflowEventProjection>,
    },
    /// Bounded durable projection for workspace bootstrap lifecycle changes.
    WorkspaceBootstrapManifest {
        /// Identifier of the workspace being initialized.
        workspace_id: String,
        /// Lifecycle operation that produced this projection.
        operation: String,
        /// Current operation status.
        status: String,
        /// Identifier of the manifest record.
        manifest_id: String,
        /// Current manifest revision.
        revision: u64,
        /// Digest of the manifest content.
        content_hash: String,
        /// Bounded JSON projection; it omits raw workspace content.
        projection_json: String,
    },
    /// Reports a cache operation and its bounded public result.
    PolicyAwareToolResultCache {
        /// Operation completed against the policy-aware cache.
        operation: String,
        /// Cache entry identifier.
        cache_key: String,
        /// Persisted version after the operation.
        version: u64,
        /// Bounded result projection; raw tool payloads are excluded.
        projection_json: String,
    },
    /// Reports a code-anchored intent marker operation.
    CodeAnchoredIntentMarkers {
        /// Operation completed against the marker registry.
        operation: String,
        /// Current marker revision.
        version: u64,
        /// Bounded marker projection without full source contents.
        projection_json: String,
    },
    /// Reports a model-purpose routing operation.
    ModelPurposeRouting {
        /// Operation completed by the purpose router.
        operation: String,
        /// Current routing policy version.
        version: u64,
        /// Bounded route and policy projection.
        projection_json: String,
    },
    /// Reports a local model runtime lifecycle operation.
    LocalModelRuntimeManager {
        /// Operation completed by the local model manager.
        operation: String,
        /// Current persisted model runtime version.
        version: u64,
        /// Bounded model status projection; credentials and model bodies are excluded.
        projection_json: String,
    },
    /// Reports creation or inspection of a workspace architecture snapshot.
    ArchitectureSnapshot {
        /// Identifier of the snapshot.
        snapshot_id: String,
        /// Snapshot operation that completed.
        operation: String,
        /// Current snapshot version.
        version: u64,
        /// Bounded metadata projection of the snapshot.
        projection_json: String,
    },
    /// Reports a typed task handoff contract operation.
    TypedAgentHandoffContract {
        /// Identifier of the handoff.
        handoff_id: String,
        /// Operation completed against the handoff contract.
        operation: String,
        /// Current handoff state.
        state: String,
        /// Persisted handoff version.
        version: u64,
        /// Bounded handoff metadata; no child output is included.
        projection_json: String,
    },
    /// Reports a declarative agent configuration operation.
    SchemaDrivenAgentConfiguration {
        /// Configuration scope.
        scope: String,
        /// Configuration operation that completed.
        operation: String,
        /// Persisted configuration revision.
        revision: u64,
        /// Bounded configuration projection.
        projection_json: String,
    },
    /// Reports an experience replay library operation.
    ExperienceReplayLibrary {
        /// Library scope.
        scope: String,
        /// Library operation that completed.
        operation: String,
        /// Persisted library revision.
        revision: u64,
        /// Bounded experience metadata projection.
        projection_json: String,
    },
    /// Reports a runtime intervention operation for one run.
    RuntimeInterventionPipeline {
        /// Run affected by the operation.
        run_id: String,
        /// Intervention operation that completed.
        operation: String,
        /// Bounded intervention status projection.
        projection_json: String,
    },
    /// Reports a workspace diagnostics feedback operation.
    CodeDiagnosticsFeedbackLoop {
        /// Workspace root associated with the diagnostic feedback.
        workspace_root_id: String,
        /// Feedback operation that completed.
        operation: String,
        /// Persisted feedback revision.
        revision: u64,
        /// Bounded feedback projection without raw source text.
        projection_json: String,
    },
    /// Reports an operation in a code review lane.
    CodeReviewLane {
        /// Identifier of the review.
        review_id: String,
        /// Review operation that completed.
        operation: String,
        /// Persisted review revision.
        revision: u64,
        /// Bounded review state projection.
        projection_json: String,
    },
    /// Reports progress or a terminal state for a multi-reviewer ensemble.
    MultiReviewerEnsemble {
        /// Identifier of the reviewer ensemble.
        ensemble_id: String,
        /// Ensemble operation that completed.
        operation: String,
        /// Persisted ensemble revision.
        revision: u64,
        /// Bounded reviewer status projection; reviewer prompts are excluded.
        projection_json: String,
    },
    /// Reports a language intelligence operation.
    LanguageIntelligence {
        /// Identifier of the language analysis request.
        request_id: String,
        /// Analysis operation that completed.
        operation: String,
        /// Persisted analysis revision, when the operation changes state.
        revision: u64,
        /// Bounded language-analysis result projection.
        projection_json: String,
    },
    /// Reports a static analysis pack operation.
    StaticAnalysisPacks {
        /// Identifier of the analysis pack.
        pack_id: String,
        /// Pack operation that completed.
        operation: String,
        /// Persisted pack revision.
        revision: u64,
        /// Bounded pack status and findings projection.
        projection_json: String,
    },
    /// Reports a context loadout profile operation.
    ContextLoadouts {
        /// Identifier of the loadout profile.
        profile_id: String,
        /// Profile operation that completed.
        operation: String,
        /// Persisted profile revision.
        revision: u64,
        /// Bounded profile projection.
        projection_json: String,
    },
    /// Reports installation or source lifecycle changes for a skill.
    SkillSourceLifecycle {
        /// Identifier of the skill installation.
        installation_id: String,
        /// Lifecycle operation that completed.
        operation: String,
        /// Persisted installation revision.
        revision: u64,
        /// Bounded installation metadata projection.
        projection_json: String,
    },
    /// Reports an operation against the kernel capability facade.
    KernelCapabilityFacade {
        /// Identifier of the capability record.
        record_id: String,
        /// Capability operation that completed.
        operation: String,
        /// Persisted capability revision.
        revision: u64,
        /// Bounded capability metadata projection.
        projection_json: String,
    },
    /// Reports a security assessment lifecycle operation.
    AuthorizedSecurityAssessment {
        /// Identifier of the security assessment.
        assessment_id: String,
        /// Assessment operation that completed.
        operation: String,
        /// Persisted assessment revision.
        revision: u64,
        /// Bounded assessment state; sensitive evidence is omitted.
        projection_json: String,
    },
    /// Reports a runtime service graph operation.
    RuntimeServiceGraph {
        /// Identifier of the service graph.
        graph_id: String,
        /// Graph operation that completed.
        operation: String,
        /// Persisted graph revision.
        revision: u64,
        /// Bounded graph projection.
        projection_json: String,
    },
    /// Reports an agent program optimizer operation.
    AgentProgramOptimizer {
        /// Identifier of the agent program.
        program_id: String,
        /// Optimizer operation that completed.
        operation: String,
        /// Persisted program revision.
        revision: u64,
        /// Bounded program metadata projection.
        projection_json: String,
    },
    /// Reports an operation on a project's knowledge notebook.
    ProjectKnowledgeNotebook {
        /// Identifier of the notebook.
        notebook_id: String,
        /// Notebook operation that completed.
        operation: String,
        /// Persisted notebook revision.
        revision: u64,
        /// Bounded notebook metadata projection without full document bodies.
        projection_json: String,
    },
    /// Reports a Git remote publication protocol operation.
    GitRemotePublicationProtocol {
        /// Identifier of the publication protocol record.
        protocol_id: String,
        /// Publication operation that completed.
        operation: String,
        /// Persisted protocol revision.
        revision: u64,
        /// Bounded publication status projection.
        projection_json: String,
    },
    /// Reports a voice input or dictation profile operation.
    VoiceInputDictation {
        /// Identifier of the dictation profile.
        profile_id: String,
        /// Operation completed by the voice input subsystem.
        operation: String,
        /// Persisted profile revision.
        revision: u64,
        /// Bounded dictation status projection without raw audio.
        projection_json: String,
    },
    /// Reports an offline experience consolidation cycle operation.
    OfflineExperienceConsolidation {
        /// Identifier of the consolidation cycle.
        cycle_id: String,
        /// Cycle operation that completed.
        operation: String,
        /// Persisted cycle revision.
        revision: u64,
        /// Bounded cycle status projection.
        projection_json: String,
    },
    /// Reports a deterministic review execution plan operation.
    DeterministicReviewExecutionPlan {
        /// Identifier of the execution plan.
        plan_id: String,
        /// Plan operation that completed.
        operation: String,
        /// Persisted plan revision.
        revision: u64,
        /// Bounded plan status projection.
        projection_json: String,
    },
    /// Reports a workflow optimization run operation.
    WorkflowOptimizationLab {
        /// Identifier of the workflow optimization run.
        run_id: String,
        /// Operation completed by the optimizer.
        operation: String,
        /// Persisted run revision.
        revision: u64,
        /// Bounded optimization status projection.
        projection_json: String,
    },
    /// Reports changes to Core topic subscriptions.
    CoreTopicSubscriptionEventBus {
        /// Subscription operation that completed.
        operation: String,
        /// Bounded subscription/event-bus projection.
        projection_json: String,
    },
    /// Reports a dependency-aware task graph operation.
    DependencyAwareTaskGraph {
        /// Identifier of the task graph.
        graph_id: String,
        /// Graph operation that completed.
        operation: String,
        /// Persisted graph revision.
        revision: u64,
        /// Bounded graph projection including validated grant state.
        projection_json: String,
    },
    /// Reports a declarative component registry operation.
    DeclarativeAgentComponentRegistry {
        /// Identifier of the component registry entry.
        registry_id: String,
        /// Registry operation that completed.
        operation: String,
        /// Persisted registry revision.
        revision: u64,
        /// Bounded component metadata projection.
        projection_json: String,
    },
    /// Reports a typed context reference operation.
    TypedContextReferences {
        /// Identifier of the context reference.
        ref_id: String,
        /// Reference operation that completed.
        operation: String,
        /// Bounded reference metadata projection without referenced content.
        projection_json: String,
    },
    /// Reports validation or lifecycle changes for a safe UI extension.
    SafeUiExtensionFramework {
        /// Identifier of the extension.
        extension_id: String,
        /// Extension operation that completed.
        operation: String,
        /// Persisted extension revision.
        revision: u64,
        /// Bounded extension and policy projection.
        projection_json: String,
    },
    /// Reports a capability workbench operation.
    CapabilityWorkbench {
        /// Identifier of the workbench instance.
        instance_id: String,
        /// Workbench operation that completed.
        operation: String,
        /// Persisted instance revision.
        revision: u64,
        /// Bounded workbench state projection.
        projection_json: String,
    },
    /// Reports a team coordinator operation for a work item.
    TeamCoordinator {
        /// Work item affected by coordination.
        work_item_id: String,
        /// Coordination operation that completed.
        operation: String,
        /// Persisted coordination revision.
        revision: u64,
        /// Bounded work-item coordination projection.
        projection_json: String,
    },
    /// Reports a project instruction stack operation.
    ProjectInstructionStack {
        /// Workspace root owning the instruction stack.
        workspace_root: String,
        /// Instruction operation that completed.
        operation: String,
        /// Persisted instruction revision.
        revision: u64,
        /// Bounded instruction metadata projection.
        projection_json: String,
    },
    /// Reports an operation on a named workspace set.
    WorkspaceSets {
        /// Identifier of the workspace set.
        set_id: String,
        /// Set operation that completed.
        operation: String,
        /// Persisted set version.
        version: u64,
        /// Bounded workspace-set projection.
        projection_json: String,
    },
    /// Reports a knowledge source/project role association operation.
    KnowledgeSourceRegistryProjectRole {
        /// Identifier of the associated source.
        source_id: String,
        /// Association operation that completed.
        operation: String,
        /// Persisted association version.
        version: u64,
        /// Bounded source-role projection.
        projection_json: String,
    },
    /// Reports a durable remote task bridge operation.
    DurableRemoteTaskBridge {
        /// Identifier of the remote task.
        remote_task_id: String,
        /// Bridge operation that completed.
        operation: String,
        /// Persisted bridge version.
        version: u64,
        /// Bounded remote task status projection; remote payloads are excluded.
        projection_json: String,
    },
    /// Reports a message intervention policy operation.
    MessageInterventionPolicies {
        /// Policy operation that completed.
        operation: String,
        /// Persisted policy version.
        version: u64,
        /// Bounded policy decision or state projection.
        projection_json: String,
    },
    /// Reports execution progress or completion for a batch invocation.
    BatchInvocationRuntime {
        /// Identifier of the invocation batch.
        batch_id: String,
        /// Batch operation that completed.
        operation: String,
        /// Persisted batch version.
        version: u64,
        /// Bounded batch state projection.
        projection_json: String,
    },
    /// Reports an agent Git change set operation.
    AgentGitChangeSets {
        /// Identifier of the change set.
        change_set_id: String,
        /// Change set operation that completed.
        operation: String,
        /// Persisted change set version.
        version: u64,
        /// Bounded change set status without raw patch contents.
        projection_json: String,
    },
    /// Reports an architect/editor/model pipeline operation.
    ArchitectEditorModelPipeline {
        /// Identifier of the pipeline.
        pipeline_id: String,
        /// Pipeline operation that completed.
        operation: String,
        /// Persisted pipeline version.
        version: u64,
        /// Bounded pipeline state projection.
        projection_json: String,
    },
    /// Reports an event visualizer registry operation.
    EventVisualizerRegistry {
        /// Identifier of the visualizer.
        visualizer_id: String,
        /// Registry operation that completed.
        operation: String,
        /// Persisted registry version.
        version: u64,
        /// Bounded visualizer metadata projection.
        projection_json: String,
    },
    /// Reports an operation in the reasoning operator library.
    ReasoningOperatorLibrary {
        /// Identifier of the reasoning operator.
        operator_id: String,
        /// Library operation that completed.
        operation: String,
        /// Persisted operator version.
        version: u64,
        /// Bounded operator metadata projection.
        projection_json: String,
    },
    /// Reports an output guardrail pipeline operation.
    OutputGuardrailPipeline {
        /// Identifier of the guardrail pipeline.
        pipeline_id: String,
        /// Pipeline operation that completed.
        operation: String,
        /// Persisted pipeline version.
        version: u64,
        /// Bounded guardrail state; protected output is excluded.
        projection_json: String,
    },
    /// Reports a customization inventory operation.
    CustomizationInventory {
        /// Identifier of the customized item.
        item_id: String,
        /// Inventory operation that completed.
        operation: String,
        /// Persisted inventory version.
        version: u64,
        /// Bounded customization metadata projection.
        projection_json: String,
    },
    /// Reports a standing approval profile operation.
    StandingApprovalProfiles {
        /// Identifier of the approval profile.
        profile_id: String,
        /// Profile operation that completed.
        operation: String,
        /// Persisted profile version.
        version: u64,
        /// Bounded profile state projection.
        projection_json: String,
    },
    /// Reports an approval policy profile operation.
    ApprovalPolicyProfiles {
        /// Identifier of the policy profile.
        profile_id: String,
        /// Profile operation that completed.
        operation: String,
        /// Persisted policy version.
        version: u64,
        /// Bounded policy state projection.
        projection_json: String,
    },
    /// Reports a checkpoint fork operation.
    CheckpointForking {
        /// Identifier of the forked run.
        fork_run_id: String,
        /// Fork operation that completed.
        operation: String,
        /// Persisted fork version.
        version: u64,
        /// Bounded fork metadata projection.
        projection_json: String,
    },
    /// Reports a privacy or telemetry governance decision.
    PrivacyTelemetryGovernance {
        /// Governance operation that completed.
        operation: String,
        /// Category of data or telemetry evaluated.
        category: String,
        /// Persisted governance policy version.
        version: u64,
        /// Bounded decision projection; event payloads are excluded.
        projection_json: String,
    },
    /// Reports a conversation channel adapter operation.
    ConversationBridgeAdapters {
        /// Adapter operation that completed.
        operation: String,
        /// Identifier of the conversation bridge.
        bridge_id: String,
        /// Persisted bridge revision.
        revision: u64,
        /// Bounded adapter status projection.
        projection_json: String,
    },
    /// Reports a memory view or adaptive recall operation.
    MemoryViewsAndAdaptiveRecall {
        /// Recall or view operation that completed.
        operation: String,
        /// Identifier of the memory view.
        view_id: String,
        /// Persisted view version.
        version: u64,
        /// Bounded recall decision projection without memory bodies.
        projection_json: String,
    },
    /// Reports a model edit protocol registry operation.
    ModelEditProtocolRegistry {
        /// Registry operation that completed.
        operation: String,
        /// Identifier of the model edit protocol.
        protocol_id: String,
        /// Persisted protocol version.
        version: u64,
        /// Bounded protocol metadata projection.
        projection_json: String,
    },
    /// Reports a remote conversation channel operation.
    RemoteConversationChannels {
        /// Channel operation that completed.
        operation: String,
        /// Identifier of the remote connection.
        connection_id: String,
        /// Persisted connection version.
        version: u64,
        /// Bounded connection status projection without credentials.
        projection_json: String,
    },
    /// Reports a prompt cache planning operation.
    PromptCachePlanner {
        /// Planner operation that completed.
        operation: String,
        /// Identifier of the prompt cache plan.
        plan_id: String,
        /// Persisted plan version.
        version: u64,
        /// Bounded cache plan projection.
        projection_json: String,
    },
    /// Reports a declarative runtime component operation.
    DeclarativeRuntimeComponents {
        /// Component operation that completed.
        operation: String,
        /// Identifier of the runtime component.
        component_id: String,
        /// Persisted component version.
        version: u64,
        /// Bounded runtime component projection.
        projection_json: String,
    },
    /// Reports a guided calibration session operation.
    GuidedCalibrationSessions {
        /// Calibration operation that completed.
        operation: String,
        /// Identifier of the calibration session.
        session_id: String,
        /// Persisted session version.
        version: u64,
        /// Bounded calibration status projection.
        projection_json: String,
    },
    /// Reports an extension conformance check.
    ExtensionConformanceKit {
        /// Conformance operation that completed.
        operation: String,
        /// Identifier of the extension or subject checked.
        subject_id: String,
        /// Persisted conformance version.
        version: u64,
        /// Bounded conformance result projection.
        projection_json: String,
    },
    /// Reports an agent organization registry operation.
    PersistentAgentOrganizationRegistry {
        /// Identifier of the agent.
        agent_id: String,
        /// Registry operation that completed.
        operation: String,
        /// Persisted registry revision.
        revision: u64,
        /// Bounded organization metadata projection.
        projection_json: String,
    },
    /// Reports an execution environment profile operation.
    ExecutionEnvironmentProfile {
        /// Identifier of the environment profile.
        profile_id: String,
        /// Profile operation that completed.
        operation: String,
        /// Persisted profile revision.
        revision: u64,
        /// Bounded environment configuration projection.
        projection_json: String,
    },
    /// Reports a context namespace operation.
    ContextNamespace {
        /// Identifier of the context namespace.
        namespace_id: String,
        /// Namespace operation that completed.
        operation: String,
        /// Persisted namespace revision.
        revision: u64,
        /// Bounded namespace metadata projection.
        projection_json: String,
    },
    /// Reports a durable background execution operation.
    DurableBackgroundExecution {
        /// Identifier of the background run.
        run_id: String,
        /// Execution operation that completed.
        operation: String,
        /// Persisted run revision.
        revision: u64,
        /// Bounded execution status projection without raw tool payloads.
        projection_json: String,
    },
    /// Bounded durable routing decision projection.
    TeamCoordinationPolicies {
        /// Identifier of the team whose coordination policy changed.
        team_id: String,
        /// Policy operation that completed.
        operation: String,
        /// Resulting coordination status.
        status: String,
        /// Persisted policy version.
        version: u64,
        /// Bounded team policy projection.
        projection_json: String,
    },
    /// Marks the point after which review history is shown. The journal is
    /// append-only, so clearing hides earlier reviews instead of deleting them.
    ReviewHistoryCleared {
        /// Durable marker separating future review history from older entries.
        marker_id: String,
    },
}
