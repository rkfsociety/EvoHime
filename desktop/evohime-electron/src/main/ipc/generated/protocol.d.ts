// GENERATED FILE - DO NOT EDIT.
// Source: crates/desktop-ipc/proto/evohime.desktop.proto
// Regenerate with: npm run generate:protocol
import * as $protobuf from "protobufjs";
import Long = require("long");

/** Namespace evohime. */
export namespace evohime {

    /** Namespace desktop. */
    namespace desktop {

        /** Namespace v1. */
        namespace v1 {

            /**
             * Properties of a ProtocolVersion.
             * @deprecated Use evohime.desktop.v1.ProtocolVersion.$Properties instead.
             */
            interface IProtocolVersion extends evohime.desktop.v1.ProtocolVersion.$Properties {
            }

            /** Represents a ProtocolVersion. */
            class ProtocolVersion {

                /**
                 * Constructs a new ProtocolVersion.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ProtocolVersion.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ProtocolVersion major. */
                major: number;

                /** ProtocolVersion minor. */
                minor: number;

                /**
                 * Encodes the specified ProtocolVersion message. Does not implicitly {@link evohime.desktop.v1.ProtocolVersion.verify|verify} messages.
                 * @param message ProtocolVersion message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ProtocolVersion.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ProtocolVersion message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ProtocolVersion & evohime.desktop.v1.ProtocolVersion.$Shape} ProtocolVersion
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ProtocolVersion & evohime.desktop.v1.ProtocolVersion.$Shape;

                /**
                 * Gets the type url for ProtocolVersion
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ProtocolVersion {

                /** Properties of a ProtocolVersion. */
                interface $Properties {

                    /** ProtocolVersion major */
                    major?: (number|null);

                    /** ProtocolVersion minor */
                    minor?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ProtocolVersion. */
                type $Shape = evohime.desktop.v1.ProtocolVersion.$Properties;
            }

            /**
             * Properties of a ProtocolOffer.
             * @deprecated Use evohime.desktop.v1.ProtocolOffer.$Properties instead.
             */
            interface IProtocolOffer extends evohime.desktop.v1.ProtocolOffer.$Properties {
            }

            /** Represents a ProtocolOffer. */
            class ProtocolOffer {

                /**
                 * Constructs a new ProtocolOffer.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ProtocolOffer.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ProtocolOffer protocol. */
                protocol?: (evohime.desktop.v1.ProtocolVersion.$Properties|null);

                /** ProtocolOffer capabilities. */
                capabilities: string[];

                /**
                 * Encodes the specified ProtocolOffer message. Does not implicitly {@link evohime.desktop.v1.ProtocolOffer.verify|verify} messages.
                 * @param message ProtocolOffer message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ProtocolOffer.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ProtocolOffer message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ProtocolOffer & evohime.desktop.v1.ProtocolOffer.$Shape} ProtocolOffer
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ProtocolOffer & evohime.desktop.v1.ProtocolOffer.$Shape;

                /**
                 * Gets the type url for ProtocolOffer
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ProtocolOffer {

                /** Properties of a ProtocolOffer. */
                interface $Properties {

                    /** ProtocolOffer protocol */
                    protocol?: (evohime.desktop.v1.ProtocolVersion.$Properties|null);

                    /** ProtocolOffer capabilities */
                    capabilities?: (string[]|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ProtocolOffer. */
                type $Shape = evohime.desktop.v1.ProtocolOffer.$Properties;
            }

            /**
             * Properties of a Handshake.
             * @deprecated Use evohime.desktop.v1.Handshake.$Properties instead.
             */
            interface IHandshake extends evohime.desktop.v1.Handshake.$Properties {
            }

            /** Represents a Handshake. */
            class Handshake {

                /**
                 * Constructs a new Handshake.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.Handshake.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** Handshake protocol. */
                protocol?: (evohime.desktop.v1.ProtocolVersion.$Properties|null);

                /** Handshake clientId. */
                clientId: string;

                /** Handshake sessionId. */
                sessionId: string;

                /** Handshake sessionEpoch. */
                sessionEpoch: number;

                /** Handshake lastEventSequence. */
                lastEventSequence: number;

                /** Handshake capabilities. */
                capabilities: string[];

                /** Handshake clientRole. */
                clientRole: string;

                /** Handshake nonce. */
                nonce: string;

                /** Handshake proof. */
                proof: string;

                /**
                 * Encodes the specified Handshake message. Does not implicitly {@link evohime.desktop.v1.Handshake.verify|verify} messages.
                 * @param message Handshake message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.Handshake.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a Handshake message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.Handshake & evohime.desktop.v1.Handshake.$Shape} Handshake
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.Handshake & evohime.desktop.v1.Handshake.$Shape;

                /**
                 * Gets the type url for Handshake
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace Handshake {

                /** Properties of a Handshake. */
                interface $Properties {

                    /** Handshake protocol */
                    protocol?: (evohime.desktop.v1.ProtocolVersion.$Properties|null);

                    /** Handshake clientId */
                    clientId?: (string|null);

                    /** Handshake sessionId */
                    sessionId?: (string|null);

                    /** Handshake sessionEpoch */
                    sessionEpoch?: (number|null);

                    /** Handshake lastEventSequence */
                    lastEventSequence?: (number|null);

                    /** Handshake capabilities */
                    capabilities?: (string[]|null);

                    /** Handshake clientRole */
                    clientRole?: (string|null);

                    /** Handshake nonce */
                    nonce?: (string|null);

                    /** Handshake proof */
                    proof?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a Handshake. */
                type $Shape = evohime.desktop.v1.Handshake.$Properties;
            }

            /**
             * Properties of an AuthChallenge.
             * @deprecated Use evohime.desktop.v1.AuthChallenge.$Properties instead.
             */
            interface IAuthChallenge extends evohime.desktop.v1.AuthChallenge.$Properties {
            }

            /** Represents an AuthChallenge. */
            class AuthChallenge {

                /**
                 * Constructs a new AuthChallenge.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.AuthChallenge.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** AuthChallenge nonce. */
                nonce: string;

                /** AuthChallenge expiresAtMs. */
                expiresAtMs: number;

                /**
                 * Encodes the specified AuthChallenge message. Does not implicitly {@link evohime.desktop.v1.AuthChallenge.verify|verify} messages.
                 * @param message AuthChallenge message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.AuthChallenge.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an AuthChallenge message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.AuthChallenge & evohime.desktop.v1.AuthChallenge.$Shape} AuthChallenge
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.AuthChallenge & evohime.desktop.v1.AuthChallenge.$Shape;

                /**
                 * Gets the type url for AuthChallenge
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace AuthChallenge {

                /** Properties of an AuthChallenge. */
                interface $Properties {

                    /** AuthChallenge nonce */
                    nonce?: (string|null);

                    /** AuthChallenge expiresAtMs */
                    expiresAtMs?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an AuthChallenge. */
                type $Shape = evohime.desktop.v1.AuthChallenge.$Properties;
            }

            /**
             * Properties of a ReplayEvents.
             * @deprecated Use evohime.desktop.v1.ReplayEvents.$Properties instead.
             */
            interface IReplayEvents extends evohime.desktop.v1.ReplayEvents.$Properties {
            }

            /** Represents a ReplayEvents. */
            class ReplayEvents {

                /**
                 * Constructs a new ReplayEvents.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ReplayEvents.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ReplayEvents afterSequence. */
                afterSequence: number;

                /**
                 * Encodes the specified ReplayEvents message. Does not implicitly {@link evohime.desktop.v1.ReplayEvents.verify|verify} messages.
                 * @param message ReplayEvents message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ReplayEvents.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ReplayEvents message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ReplayEvents & evohime.desktop.v1.ReplayEvents.$Shape} ReplayEvents
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ReplayEvents & evohime.desktop.v1.ReplayEvents.$Shape;

                /**
                 * Gets the type url for ReplayEvents
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ReplayEvents {

                /** Properties of a ReplayEvents. */
                interface $Properties {

                    /** ReplayEvents afterSequence */
                    afterSequence?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ReplayEvents. */
                type $Shape = evohime.desktop.v1.ReplayEvents.$Properties;
            }

            /**
             * Properties of a ResyncRequest.
             * @deprecated Use evohime.desktop.v1.ResyncRequest.$Properties instead.
             */
            interface IResyncRequest extends evohime.desktop.v1.ResyncRequest.$Properties {
            }

            /** Represents a ResyncRequest. */
            class ResyncRequest {

                /**
                 * Constructs a new ResyncRequest.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ResyncRequest.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ResyncRequest afterSequence. */
                afterSequence: number;

                /** ResyncRequest maxEvents. */
                maxEvents: number;

                /** ResyncRequest includeFullSnapshot. */
                includeFullSnapshot: boolean;

                /**
                 * Encodes the specified ResyncRequest message. Does not implicitly {@link evohime.desktop.v1.ResyncRequest.verify|verify} messages.
                 * @param message ResyncRequest message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ResyncRequest.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ResyncRequest message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ResyncRequest & evohime.desktop.v1.ResyncRequest.$Shape} ResyncRequest
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ResyncRequest & evohime.desktop.v1.ResyncRequest.$Shape;

                /**
                 * Gets the type url for ResyncRequest
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ResyncRequest {

                /** Properties of a ResyncRequest. */
                interface $Properties {

                    /** ResyncRequest afterSequence */
                    afterSequence?: (number|null);

                    /** ResyncRequest maxEvents */
                    maxEvents?: (number|null);

                    /** ResyncRequest includeFullSnapshot */
                    includeFullSnapshot?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ResyncRequest. */
                type $Shape = evohime.desktop.v1.ResyncRequest.$Properties;
            }

            /**
             * Properties of a ModelConfigRequest.
             * @deprecated Use evohime.desktop.v1.ModelConfigRequest.$Properties instead.
             */
            interface IModelConfigRequest extends evohime.desktop.v1.ModelConfigRequest.$Properties {
            }

            /** Represents a ModelConfigRequest. */
            class ModelConfigRequest {

                /**
                 * Constructs a new ModelConfigRequest.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ModelConfigRequest.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /**
                 * Encodes the specified ModelConfigRequest message. Does not implicitly {@link evohime.desktop.v1.ModelConfigRequest.verify|verify} messages.
                 * @param message ModelConfigRequest message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ModelConfigRequest.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ModelConfigRequest message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ModelConfigRequest & evohime.desktop.v1.ModelConfigRequest.$Shape} ModelConfigRequest
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ModelConfigRequest & evohime.desktop.v1.ModelConfigRequest.$Shape;

                /**
                 * Gets the type url for ModelConfigRequest
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ModelConfigRequest {

                /** Properties of a ModelConfigRequest. */
                interface $Properties {

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ModelConfigRequest. */
                type $Shape = evohime.desktop.v1.ModelConfigRequest.$Properties;
            }

            /**
             * Properties of a ModelCatalogRequest.
             * @deprecated Use evohime.desktop.v1.ModelCatalogRequest.$Properties instead.
             */
            interface IModelCatalogRequest extends evohime.desktop.v1.ModelCatalogRequest.$Properties {
            }

            /** Represents a ModelCatalogRequest. */
            class ModelCatalogRequest {

                /**
                 * Constructs a new ModelCatalogRequest.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ModelCatalogRequest.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ModelCatalogRequest mode. */
                mode: string;

                /**
                 * Encodes the specified ModelCatalogRequest message. Does not implicitly {@link evohime.desktop.v1.ModelCatalogRequest.verify|verify} messages.
                 * @param message ModelCatalogRequest message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ModelCatalogRequest.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ModelCatalogRequest message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ModelCatalogRequest & evohime.desktop.v1.ModelCatalogRequest.$Shape} ModelCatalogRequest
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ModelCatalogRequest & evohime.desktop.v1.ModelCatalogRequest.$Shape;

                /**
                 * Gets the type url for ModelCatalogRequest
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ModelCatalogRequest {

                /** Properties of a ModelCatalogRequest. */
                interface $Properties {

                    /** ModelCatalogRequest mode */
                    mode?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ModelCatalogRequest. */
                type $Shape = evohime.desktop.v1.ModelCatalogRequest.$Properties;
            }

            /**
             * Properties of a SelectModelRequest.
             * @deprecated Use evohime.desktop.v1.SelectModelRequest.$Properties instead.
             */
            interface ISelectModelRequest extends evohime.desktop.v1.SelectModelRequest.$Properties {
            }

            /** Represents a SelectModelRequest. */
            class SelectModelRequest {

                /**
                 * Constructs a new SelectModelRequest.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SelectModelRequest.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SelectModelRequest model. */
                model: string;

                /**
                 * Encodes the specified SelectModelRequest message. Does not implicitly {@link evohime.desktop.v1.SelectModelRequest.verify|verify} messages.
                 * @param message SelectModelRequest message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SelectModelRequest.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SelectModelRequest message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SelectModelRequest & evohime.desktop.v1.SelectModelRequest.$Shape} SelectModelRequest
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SelectModelRequest & evohime.desktop.v1.SelectModelRequest.$Shape;

                /**
                 * Gets the type url for SelectModelRequest
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SelectModelRequest {

                /** Properties of a SelectModelRequest. */
                interface $Properties {

                    /** SelectModelRequest model */
                    model?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SelectModelRequest. */
                type $Shape = evohime.desktop.v1.SelectModelRequest.$Properties;
            }

            /**
             * Properties of a StartPlanReview.
             * @deprecated Use evohime.desktop.v1.StartPlanReview.$Properties instead.
             */
            interface IStartPlanReview extends evohime.desktop.v1.StartPlanReview.$Properties {
            }

            /** Represents a StartPlanReview. */
            class StartPlanReview {

                /**
                 * Constructs a new StartPlanReview.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.StartPlanReview.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** StartPlanReview reviewId. */
                reviewId: string;

                /** StartPlanReview fileName. */
                fileName: string;

                /** StartPlanReview sourceMarkdown. */
                sourceMarkdown: string;

                /** StartPlanReview reviewerModels. */
                reviewerModels: string[];

                /** StartPlanReview synthesisModel. */
                synthesisModel: string;

                /** StartPlanReview fileNames. */
                fileNames: string[];

                /** StartPlanReview sourcePaths. */
                sourcePaths: string[];

                /**
                 * Encodes the specified StartPlanReview message. Does not implicitly {@link evohime.desktop.v1.StartPlanReview.verify|verify} messages.
                 * @param message StartPlanReview message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.StartPlanReview.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a StartPlanReview message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.StartPlanReview & evohime.desktop.v1.StartPlanReview.$Shape} StartPlanReview
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.StartPlanReview & evohime.desktop.v1.StartPlanReview.$Shape;

                /**
                 * Gets the type url for StartPlanReview
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace StartPlanReview {

                /** Properties of a StartPlanReview. */
                interface $Properties {

                    /** StartPlanReview reviewId */
                    reviewId?: (string|null);

                    /** StartPlanReview fileName */
                    fileName?: (string|null);

                    /** StartPlanReview sourceMarkdown */
                    sourceMarkdown?: (string|null);

                    /** StartPlanReview reviewerModels */
                    reviewerModels?: (string[]|null);

                    /** StartPlanReview synthesisModel */
                    synthesisModel?: (string|null);

                    /** StartPlanReview fileNames */
                    fileNames?: (string[]|null);

                    /** StartPlanReview sourcePaths */
                    sourcePaths?: (string[]|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a StartPlanReview. */
                type $Shape = evohime.desktop.v1.StartPlanReview.$Properties;
            }

            /**
             * Properties of a StopPlanReview.
             * @deprecated Use evohime.desktop.v1.StopPlanReview.$Properties instead.
             */
            interface IStopPlanReview extends evohime.desktop.v1.StopPlanReview.$Properties {
            }

            /** Represents a StopPlanReview. */
            class StopPlanReview {

                /**
                 * Constructs a new StopPlanReview.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.StopPlanReview.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** StopPlanReview reviewId. */
                reviewId: string;

                /**
                 * Encodes the specified StopPlanReview message. Does not implicitly {@link evohime.desktop.v1.StopPlanReview.verify|verify} messages.
                 * @param message StopPlanReview message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.StopPlanReview.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a StopPlanReview message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.StopPlanReview & evohime.desktop.v1.StopPlanReview.$Shape} StopPlanReview
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.StopPlanReview & evohime.desktop.v1.StopPlanReview.$Shape;

                /**
                 * Gets the type url for StopPlanReview
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace StopPlanReview {

                /** Properties of a StopPlanReview. */
                interface $Properties {

                    /** StopPlanReview reviewId */
                    reviewId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a StopPlanReview. */
                type $Shape = evohime.desktop.v1.StopPlanReview.$Properties;
            }

            /**
             * Properties of a ListPlanReviews.
             * @deprecated Use evohime.desktop.v1.ListPlanReviews.$Properties instead.
             */
            interface IListPlanReviews extends evohime.desktop.v1.ListPlanReviews.$Properties {
            }

            /** Represents a ListPlanReviews. */
            class ListPlanReviews {

                /**
                 * Constructs a new ListPlanReviews.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ListPlanReviews.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ListPlanReviews limit. */
                limit: number;

                /**
                 * Encodes the specified ListPlanReviews message. Does not implicitly {@link evohime.desktop.v1.ListPlanReviews.verify|verify} messages.
                 * @param message ListPlanReviews message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ListPlanReviews.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ListPlanReviews message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListPlanReviews & evohime.desktop.v1.ListPlanReviews.$Shape} ListPlanReviews
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ListPlanReviews & evohime.desktop.v1.ListPlanReviews.$Shape;

                /**
                 * Gets the type url for ListPlanReviews
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ListPlanReviews {

                /** Properties of a ListPlanReviews. */
                interface $Properties {

                    /** ListPlanReviews limit */
                    limit?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ListPlanReviews. */
                type $Shape = evohime.desktop.v1.ListPlanReviews.$Properties;
            }

            /**
             * Properties of a GetPlanReview.
             * @deprecated Use evohime.desktop.v1.GetPlanReview.$Properties instead.
             */
            interface IGetPlanReview extends evohime.desktop.v1.GetPlanReview.$Properties {
            }

            /** Represents a GetPlanReview. */
            class GetPlanReview {

                /**
                 * Constructs a new GetPlanReview.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetPlanReview.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GetPlanReview reviewId. */
                reviewId: string;

                /**
                 * Encodes the specified GetPlanReview message. Does not implicitly {@link evohime.desktop.v1.GetPlanReview.verify|verify} messages.
                 * @param message GetPlanReview message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetPlanReview.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetPlanReview message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetPlanReview & evohime.desktop.v1.GetPlanReview.$Shape} GetPlanReview
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetPlanReview & evohime.desktop.v1.GetPlanReview.$Shape;

                /**
                 * Gets the type url for GetPlanReview
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetPlanReview {

                /** Properties of a GetPlanReview. */
                interface $Properties {

                    /** GetPlanReview reviewId */
                    reviewId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetPlanReview. */
                type $Shape = evohime.desktop.v1.GetPlanReview.$Properties;
            }

            /**
             * Properties of an ExportPlanReview.
             * @deprecated Use evohime.desktop.v1.ExportPlanReview.$Properties instead.
             */
            interface IExportPlanReview extends evohime.desktop.v1.ExportPlanReview.$Properties {
            }

            /** Represents an ExportPlanReview. */
            class ExportPlanReview {

                /**
                 * Constructs a new ExportPlanReview.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ExportPlanReview.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ExportPlanReview reviewId. */
                reviewId: string;

                /** ExportPlanReview destinationPath. */
                destinationPath: string;

                /** ExportPlanReview includeReviewers. */
                includeReviewers: boolean;

                /**
                 * Encodes the specified ExportPlanReview message. Does not implicitly {@link evohime.desktop.v1.ExportPlanReview.verify|verify} messages.
                 * @param message ExportPlanReview message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ExportPlanReview.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an ExportPlanReview message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ExportPlanReview & evohime.desktop.v1.ExportPlanReview.$Shape} ExportPlanReview
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ExportPlanReview & evohime.desktop.v1.ExportPlanReview.$Shape;

                /**
                 * Gets the type url for ExportPlanReview
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ExportPlanReview {

                /** Properties of an ExportPlanReview. */
                interface $Properties {

                    /** ExportPlanReview reviewId */
                    reviewId?: (string|null);

                    /** ExportPlanReview destinationPath */
                    destinationPath?: (string|null);

                    /** ExportPlanReview includeReviewers */
                    includeReviewers?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an ExportPlanReview. */
                type $Shape = evohime.desktop.v1.ExportPlanReview.$Properties;
            }

            /**
             * Properties of a ClearPlanReviewHistory.
             * @deprecated Use evohime.desktop.v1.ClearPlanReviewHistory.$Properties instead.
             */
            interface IClearPlanReviewHistory extends evohime.desktop.v1.ClearPlanReviewHistory.$Properties {
            }

            /** Represents a ClearPlanReviewHistory. */
            class ClearPlanReviewHistory {

                /**
                 * Constructs a new ClearPlanReviewHistory.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ClearPlanReviewHistory.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /**
                 * Encodes the specified ClearPlanReviewHistory message. Does not implicitly {@link evohime.desktop.v1.ClearPlanReviewHistory.verify|verify} messages.
                 * @param message ClearPlanReviewHistory message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ClearPlanReviewHistory.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ClearPlanReviewHistory message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ClearPlanReviewHistory & evohime.desktop.v1.ClearPlanReviewHistory.$Shape} ClearPlanReviewHistory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ClearPlanReviewHistory & evohime.desktop.v1.ClearPlanReviewHistory.$Shape;

                /**
                 * Gets the type url for ClearPlanReviewHistory
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ClearPlanReviewHistory {

                /** Properties of a ClearPlanReviewHistory. */
                interface $Properties {

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ClearPlanReviewHistory. */
                type $Shape = evohime.desktop.v1.ClearPlanReviewHistory.$Properties;
            }

            /**
             * Properties of a RevisePlan.
             * @deprecated Use evohime.desktop.v1.RevisePlan.$Properties instead.
             */
            interface IRevisePlan extends evohime.desktop.v1.RevisePlan.$Properties {
            }

            /** Represents a RevisePlan. */
            class RevisePlan {

                /**
                 * Constructs a new RevisePlan.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.RevisePlan.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** RevisePlan revisionId. */
                revisionId: string;

                /** RevisePlan reviewId. */
                reviewId: string;

                /** RevisePlan fileName. */
                fileName: string;

                /** RevisePlan sourceMarkdown. */
                sourceMarkdown: string;

                /** RevisePlan model. */
                model: string;

                /** RevisePlan sourcePath. */
                sourcePath: string;

                /**
                 * Encodes the specified RevisePlan message. Does not implicitly {@link evohime.desktop.v1.RevisePlan.verify|verify} messages.
                 * @param message RevisePlan message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.RevisePlan.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a RevisePlan message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RevisePlan & evohime.desktop.v1.RevisePlan.$Shape} RevisePlan
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.RevisePlan & evohime.desktop.v1.RevisePlan.$Shape;

                /**
                 * Gets the type url for RevisePlan
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace RevisePlan {

                /** Properties of a RevisePlan. */
                interface $Properties {

                    /** RevisePlan revisionId */
                    revisionId?: (string|null);

                    /** RevisePlan reviewId */
                    reviewId?: (string|null);

                    /** RevisePlan fileName */
                    fileName?: (string|null);

                    /** RevisePlan sourceMarkdown */
                    sourceMarkdown?: (string|null);

                    /** RevisePlan model */
                    model?: (string|null);

                    /** RevisePlan sourcePath */
                    sourcePath?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a RevisePlan. */
                type $Shape = evohime.desktop.v1.RevisePlan.$Properties;
            }

            /**
             * Properties of a StopRevision.
             * @deprecated Use evohime.desktop.v1.StopRevision.$Properties instead.
             */
            interface IStopRevision extends evohime.desktop.v1.StopRevision.$Properties {
            }

            /** Represents a StopRevision. */
            class StopRevision {

                /**
                 * Constructs a new StopRevision.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.StopRevision.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** StopRevision revisionId. */
                revisionId: string;

                /**
                 * Encodes the specified StopRevision message. Does not implicitly {@link evohime.desktop.v1.StopRevision.verify|verify} messages.
                 * @param message StopRevision message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.StopRevision.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a StopRevision message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.StopRevision & evohime.desktop.v1.StopRevision.$Shape} StopRevision
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.StopRevision & evohime.desktop.v1.StopRevision.$Shape;

                /**
                 * Gets the type url for StopRevision
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace StopRevision {

                /** Properties of a StopRevision. */
                interface $Properties {

                    /** StopRevision revisionId */
                    revisionId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a StopRevision. */
                type $Shape = evohime.desktop.v1.StopRevision.$Properties;
            }

            /**
             * Properties of a SaveRevisedPlan.
             * @deprecated Use evohime.desktop.v1.SaveRevisedPlan.$Properties instead.
             */
            interface ISaveRevisedPlan extends evohime.desktop.v1.SaveRevisedPlan.$Properties {
            }

            /** Represents a SaveRevisedPlan. */
            class SaveRevisedPlan {

                /**
                 * Constructs a new SaveRevisedPlan.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SaveRevisedPlan.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SaveRevisedPlan revisionId. */
                revisionId: string;

                /** SaveRevisedPlan destinationPath. */
                destinationPath: string;

                /**
                 * Encodes the specified SaveRevisedPlan message. Does not implicitly {@link evohime.desktop.v1.SaveRevisedPlan.verify|verify} messages.
                 * @param message SaveRevisedPlan message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SaveRevisedPlan.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SaveRevisedPlan message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SaveRevisedPlan & evohime.desktop.v1.SaveRevisedPlan.$Shape} SaveRevisedPlan
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SaveRevisedPlan & evohime.desktop.v1.SaveRevisedPlan.$Shape;

                /**
                 * Gets the type url for SaveRevisedPlan
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SaveRevisedPlan {

                /** Properties of a SaveRevisedPlan. */
                interface $Properties {

                    /** SaveRevisedPlan revisionId */
                    revisionId?: (string|null);

                    /** SaveRevisedPlan destinationPath */
                    destinationPath?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SaveRevisedPlan. */
                type $Shape = evohime.desktop.v1.SaveRevisedPlan.$Properties;
            }

            /**
             * Properties of a PermissionModeRequest.
             * @deprecated Use evohime.desktop.v1.PermissionModeRequest.$Properties instead.
             */
            interface IPermissionModeRequest extends evohime.desktop.v1.PermissionModeRequest.$Properties {
            }

            /** Represents a PermissionModeRequest. */
            class PermissionModeRequest {

                /**
                 * Constructs a new PermissionModeRequest.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.PermissionModeRequest.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** PermissionModeRequest mode. */
                mode: string;

                /**
                 * Encodes the specified PermissionModeRequest message. Does not implicitly {@link evohime.desktop.v1.PermissionModeRequest.verify|verify} messages.
                 * @param message PermissionModeRequest message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.PermissionModeRequest.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a PermissionModeRequest message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.PermissionModeRequest & evohime.desktop.v1.PermissionModeRequest.$Shape} PermissionModeRequest
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.PermissionModeRequest & evohime.desktop.v1.PermissionModeRequest.$Shape;

                /**
                 * Gets the type url for PermissionModeRequest
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace PermissionModeRequest {

                /** Properties of a PermissionModeRequest. */
                interface $Properties {

                    /** PermissionModeRequest mode */
                    mode?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a PermissionModeRequest. */
                type $Shape = evohime.desktop.v1.PermissionModeRequest.$Properties;
            }

            /** TaskStatus enum. */
            enum TaskStatus {

                /** TASK_STATUS_UNKNOWN value */
                TASK_STATUS_UNKNOWN = 0,

                /** TASK_STATUS_BACKLOG value */
                TASK_STATUS_BACKLOG = 1,

                /** TASK_STATUS_READY value */
                TASK_STATUS_READY = 2,

                /** TASK_STATUS_IN_PROGRESS value */
                TASK_STATUS_IN_PROGRESS = 3,

                /** TASK_STATUS_DONE value */
                TASK_STATUS_DONE = 4
            }

            /**
             * Properties of a CreateProject.
             * @deprecated Use evohime.desktop.v1.CreateProject.$Properties instead.
             */
            interface ICreateProject extends evohime.desktop.v1.CreateProject.$Properties {
            }

            /** Represents a CreateProject. */
            class CreateProject {

                /**
                 * Constructs a new CreateProject.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.CreateProject.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** CreateProject projectId. */
                projectId: string;

                /** CreateProject title. */
                title: string;

                /** CreateProject workspacePath. */
                workspacePath: string;

                /** CreateProject sourceRef. */
                sourceRef: string;

                /**
                 * Encodes the specified CreateProject message. Does not implicitly {@link evohime.desktop.v1.CreateProject.verify|verify} messages.
                 * @param message CreateProject message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.CreateProject.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CreateProject message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.CreateProject & evohime.desktop.v1.CreateProject.$Shape} CreateProject
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.CreateProject & evohime.desktop.v1.CreateProject.$Shape;

                /**
                 * Gets the type url for CreateProject
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace CreateProject {

                /** Properties of a CreateProject. */
                interface $Properties {

                    /** CreateProject projectId */
                    projectId?: (string|null);

                    /** CreateProject title */
                    title?: (string|null);

                    /** CreateProject workspacePath */
                    workspacePath?: (string|null);

                    /** CreateProject sourceRef */
                    sourceRef?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a CreateProject. */
                type $Shape = evohime.desktop.v1.CreateProject.$Properties;
            }

            /**
             * Properties of a CreateTask.
             * @deprecated Use evohime.desktop.v1.CreateTask.$Properties instead.
             */
            interface ICreateTask extends evohime.desktop.v1.CreateTask.$Properties {
            }

            /** Represents a CreateTask. */
            class CreateTask {

                /**
                 * Constructs a new CreateTask.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.CreateTask.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** CreateTask taskId. */
                taskId: string;

                /** CreateTask projectId. */
                projectId: string;

                /** CreateTask parentId. */
                parentId: string;

                /** CreateTask title. */
                title: string;

                /** CreateTask description. */
                description: string;

                /** CreateTask sourceRef. */
                sourceRef: string;

                /** CreateTask acceptanceCriteria. */
                acceptanceCriteria: string;

                /** CreateTask nonGoals. */
                nonGoals: string;

                /** CreateTask status. */
                status: string;

                /** CreateTask priority. */
                priority: number;

                /** CreateTask estimate. */
                estimate: number;

                /** CreateTask complexity. */
                complexity: string;

                /** CreateTask statusCode. */
                statusCode: evohime.desktop.v1.TaskStatus;

                /**
                 * Encodes the specified CreateTask message. Does not implicitly {@link evohime.desktop.v1.CreateTask.verify|verify} messages.
                 * @param message CreateTask message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.CreateTask.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CreateTask message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.CreateTask & evohime.desktop.v1.CreateTask.$Shape} CreateTask
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.CreateTask & evohime.desktop.v1.CreateTask.$Shape;

                /**
                 * Gets the type url for CreateTask
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace CreateTask {

                /** Properties of a CreateTask. */
                interface $Properties {

                    /** CreateTask taskId */
                    taskId?: (string|null);

                    /** CreateTask projectId */
                    projectId?: (string|null);

                    /** CreateTask parentId */
                    parentId?: (string|null);

                    /** CreateTask title */
                    title?: (string|null);

                    /** CreateTask description */
                    description?: (string|null);

                    /** CreateTask sourceRef */
                    sourceRef?: (string|null);

                    /** CreateTask acceptanceCriteria */
                    acceptanceCriteria?: (string|null);

                    /** CreateTask nonGoals */
                    nonGoals?: (string|null);

                    /** CreateTask status */
                    status?: (string|null);

                    /** CreateTask priority */
                    priority?: (number|null);

                    /** CreateTask estimate */
                    estimate?: (number|null);

                    /** CreateTask complexity */
                    complexity?: (string|null);

                    /** CreateTask statusCode */
                    statusCode?: (evohime.desktop.v1.TaskStatus|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a CreateTask. */
                type $Shape = evohime.desktop.v1.CreateTask.$Properties;
            }

            /**
             * Properties of an UpdateTaskStatus.
             * @deprecated Use evohime.desktop.v1.UpdateTaskStatus.$Properties instead.
             */
            interface IUpdateTaskStatus extends evohime.desktop.v1.UpdateTaskStatus.$Properties {
            }

            /** Represents an UpdateTaskStatus. */
            class UpdateTaskStatus {

                /**
                 * Constructs a new UpdateTaskStatus.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.UpdateTaskStatus.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** UpdateTaskStatus taskId. */
                taskId: string;

                /** UpdateTaskStatus expectedVersion. */
                expectedVersion: number;

                /** UpdateTaskStatus status. */
                status: string;

                /**
                 * Encodes the specified UpdateTaskStatus message. Does not implicitly {@link evohime.desktop.v1.UpdateTaskStatus.verify|verify} messages.
                 * @param message UpdateTaskStatus message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.UpdateTaskStatus.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an UpdateTaskStatus message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.UpdateTaskStatus & evohime.desktop.v1.UpdateTaskStatus.$Shape} UpdateTaskStatus
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.UpdateTaskStatus & evohime.desktop.v1.UpdateTaskStatus.$Shape;

                /**
                 * Gets the type url for UpdateTaskStatus
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace UpdateTaskStatus {

                /** Properties of an UpdateTaskStatus. */
                interface $Properties {

                    /** UpdateTaskStatus taskId */
                    taskId?: (string|null);

                    /** UpdateTaskStatus expectedVersion */
                    expectedVersion?: (number|null);

                    /** UpdateTaskStatus status */
                    status?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an UpdateTaskStatus. */
                type $Shape = evohime.desktop.v1.UpdateTaskStatus.$Properties;
            }

            /**
             * Properties of an AddTaskEdge.
             * @deprecated Use evohime.desktop.v1.AddTaskEdge.$Properties instead.
             */
            interface IAddTaskEdge extends evohime.desktop.v1.AddTaskEdge.$Properties {
            }

            /** Represents an AddTaskEdge. */
            class AddTaskEdge {

                /**
                 * Constructs a new AddTaskEdge.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.AddTaskEdge.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** AddTaskEdge fromTaskId. */
                fromTaskId: string;

                /** AddTaskEdge toTaskId. */
                toTaskId: string;

                /** AddTaskEdge kind. */
                kind: string;

                /**
                 * Encodes the specified AddTaskEdge message. Does not implicitly {@link evohime.desktop.v1.AddTaskEdge.verify|verify} messages.
                 * @param message AddTaskEdge message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.AddTaskEdge.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an AddTaskEdge message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.AddTaskEdge & evohime.desktop.v1.AddTaskEdge.$Shape} AddTaskEdge
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.AddTaskEdge & evohime.desktop.v1.AddTaskEdge.$Shape;

                /**
                 * Gets the type url for AddTaskEdge
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace AddTaskEdge {

                /** Properties of an AddTaskEdge. */
                interface $Properties {

                    /** AddTaskEdge fromTaskId */
                    fromTaskId?: (string|null);

                    /** AddTaskEdge toTaskId */
                    toTaskId?: (string|null);

                    /** AddTaskEdge kind */
                    kind?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an AddTaskEdge. */
                type $Shape = evohime.desktop.v1.AddTaskEdge.$Properties;
            }

            /**
             * Properties of a GetTaskGraph.
             * @deprecated Use evohime.desktop.v1.GetTaskGraph.$Properties instead.
             */
            interface IGetTaskGraph extends evohime.desktop.v1.GetTaskGraph.$Properties {
            }

            /** Represents a GetTaskGraph. */
            class GetTaskGraph {

                /**
                 * Constructs a new GetTaskGraph.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetTaskGraph.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GetTaskGraph projectId. */
                projectId: string;

                /**
                 * Encodes the specified GetTaskGraph message. Does not implicitly {@link evohime.desktop.v1.GetTaskGraph.verify|verify} messages.
                 * @param message GetTaskGraph message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetTaskGraph.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetTaskGraph message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetTaskGraph & evohime.desktop.v1.GetTaskGraph.$Shape} GetTaskGraph
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetTaskGraph & evohime.desktop.v1.GetTaskGraph.$Shape;

                /**
                 * Gets the type url for GetTaskGraph
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetTaskGraph {

                /** Properties of a GetTaskGraph. */
                interface $Properties {

                    /** GetTaskGraph projectId */
                    projectId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetTaskGraph. */
                type $Shape = evohime.desktop.v1.GetTaskGraph.$Properties;
            }

            /**
             * Properties of a NextReadyTask.
             * @deprecated Use evohime.desktop.v1.NextReadyTask.$Properties instead.
             */
            interface INextReadyTask extends evohime.desktop.v1.NextReadyTask.$Properties {
            }

            /** Represents a NextReadyTask. */
            class NextReadyTask {

                /**
                 * Constructs a new NextReadyTask.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.NextReadyTask.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** NextReadyTask projectId. */
                projectId: string;

                /**
                 * Encodes the specified NextReadyTask message. Does not implicitly {@link evohime.desktop.v1.NextReadyTask.verify|verify} messages.
                 * @param message NextReadyTask message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.NextReadyTask.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a NextReadyTask message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.NextReadyTask & evohime.desktop.v1.NextReadyTask.$Shape} NextReadyTask
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.NextReadyTask & evohime.desktop.v1.NextReadyTask.$Shape;

                /**
                 * Gets the type url for NextReadyTask
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace NextReadyTask {

                /** Properties of a NextReadyTask. */
                interface $Properties {

                    /** NextReadyTask projectId */
                    projectId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a NextReadyTask. */
                type $Shape = evohime.desktop.v1.NextReadyTask.$Properties;
            }

            /**
             * Properties of an ImportPrd.
             * @deprecated Use evohime.desktop.v1.ImportPrd.$Properties instead.
             */
            interface IImportPrd extends evohime.desktop.v1.ImportPrd.$Properties {
            }

            /** Represents an ImportPrd. */
            class ImportPrd {

                /**
                 * Constructs a new ImportPrd.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ImportPrd.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ImportPrd importId. */
                importId: string;

                /** ImportPrd projectId. */
                projectId: string;

                /** ImportPrd origin. */
                origin: string;

                /** ImportPrd version. */
                version: string;

                /** ImportPrd sourceText. */
                sourceText: string;

                /**
                 * Encodes the specified ImportPrd message. Does not implicitly {@link evohime.desktop.v1.ImportPrd.verify|verify} messages.
                 * @param message ImportPrd message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ImportPrd.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an ImportPrd message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ImportPrd & evohime.desktop.v1.ImportPrd.$Shape} ImportPrd
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ImportPrd & evohime.desktop.v1.ImportPrd.$Shape;

                /**
                 * Gets the type url for ImportPrd
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ImportPrd {

                /** Properties of an ImportPrd. */
                interface $Properties {

                    /** ImportPrd importId */
                    importId?: (string|null);

                    /** ImportPrd projectId */
                    projectId?: (string|null);

                    /** ImportPrd origin */
                    origin?: (string|null);

                    /** ImportPrd version */
                    version?: (string|null);

                    /** ImportPrd sourceText */
                    sourceText?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an ImportPrd. */
                type $Shape = evohime.desktop.v1.ImportPrd.$Properties;
            }

            /**
             * Properties of a GetTaskHistory.
             * @deprecated Use evohime.desktop.v1.GetTaskHistory.$Properties instead.
             */
            interface IGetTaskHistory extends evohime.desktop.v1.GetTaskHistory.$Properties {
            }

            /** Represents a GetTaskHistory. */
            class GetTaskHistory {

                /**
                 * Constructs a new GetTaskHistory.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetTaskHistory.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GetTaskHistory taskId. */
                taskId: string;

                /** GetTaskHistory limit. */
                limit: number;

                /**
                 * Encodes the specified GetTaskHistory message. Does not implicitly {@link evohime.desktop.v1.GetTaskHistory.verify|verify} messages.
                 * @param message GetTaskHistory message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetTaskHistory.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetTaskHistory message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetTaskHistory & evohime.desktop.v1.GetTaskHistory.$Shape} GetTaskHistory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetTaskHistory & evohime.desktop.v1.GetTaskHistory.$Shape;

                /**
                 * Gets the type url for GetTaskHistory
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetTaskHistory {

                /** Properties of a GetTaskHistory. */
                interface $Properties {

                    /** GetTaskHistory taskId */
                    taskId?: (string|null);

                    /** GetTaskHistory limit */
                    limit?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetTaskHistory. */
                type $Shape = evohime.desktop.v1.GetTaskHistory.$Properties;
            }

            /**
             * Properties of a GetTaskContext.
             * @deprecated Use evohime.desktop.v1.GetTaskContext.$Properties instead.
             */
            interface IGetTaskContext extends evohime.desktop.v1.GetTaskContext.$Properties {
            }

            /** Represents a GetTaskContext. */
            class GetTaskContext {

                /**
                 * Constructs a new GetTaskContext.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetTaskContext.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GetTaskContext projectId. */
                projectId: string;

                /** GetTaskContext taskId. */
                taskId: string;

                /** GetTaskContext maxChars. */
                maxChars: number;

                /**
                 * Encodes the specified GetTaskContext message. Does not implicitly {@link evohime.desktop.v1.GetTaskContext.verify|verify} messages.
                 * @param message GetTaskContext message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetTaskContext.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetTaskContext message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetTaskContext & evohime.desktop.v1.GetTaskContext.$Shape} GetTaskContext
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetTaskContext & evohime.desktop.v1.GetTaskContext.$Shape;

                /**
                 * Gets the type url for GetTaskContext
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetTaskContext {

                /** Properties of a GetTaskContext. */
                interface $Properties {

                    /** GetTaskContext projectId */
                    projectId?: (string|null);

                    /** GetTaskContext taskId */
                    taskId?: (string|null);

                    /** GetTaskContext maxChars */
                    maxChars?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetTaskContext. */
                type $Shape = evohime.desktop.v1.GetTaskContext.$Properties;
            }

            /**
             * Properties of a GetTaskPlanSpec.
             * @deprecated Use evohime.desktop.v1.GetTaskPlanSpec.$Properties instead.
             */
            interface IGetTaskPlanSpec extends evohime.desktop.v1.GetTaskPlanSpec.$Properties {
            }

            /** Represents a GetTaskPlanSpec. */
            class GetTaskPlanSpec {

                /**
                 * Constructs a new GetTaskPlanSpec.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetTaskPlanSpec.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GetTaskPlanSpec projectId. */
                projectId: string;

                /** GetTaskPlanSpec taskId. */
                taskId: string;

                /** GetTaskPlanSpec maxChars. */
                maxChars: number;

                /**
                 * Encodes the specified GetTaskPlanSpec message. Does not implicitly {@link evohime.desktop.v1.GetTaskPlanSpec.verify|verify} messages.
                 * @param message GetTaskPlanSpec message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetTaskPlanSpec.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetTaskPlanSpec message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetTaskPlanSpec & evohime.desktop.v1.GetTaskPlanSpec.$Shape} GetTaskPlanSpec
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetTaskPlanSpec & evohime.desktop.v1.GetTaskPlanSpec.$Shape;

                /**
                 * Gets the type url for GetTaskPlanSpec
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetTaskPlanSpec {

                /** Properties of a GetTaskPlanSpec. */
                interface $Properties {

                    /** GetTaskPlanSpec projectId */
                    projectId?: (string|null);

                    /** GetTaskPlanSpec taskId */
                    taskId?: (string|null);

                    /** GetTaskPlanSpec maxChars */
                    maxChars?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetTaskPlanSpec. */
                type $Shape = evohime.desktop.v1.GetTaskPlanSpec.$Properties;
            }

            /**
             * Properties of an ApplyApprovedBuild.
             * @deprecated Use evohime.desktop.v1.ApplyApprovedBuild.$Properties instead.
             */
            interface IApplyApprovedBuild extends evohime.desktop.v1.ApplyApprovedBuild.$Properties {
            }

            /** Represents an ApplyApprovedBuild. */
            class ApplyApprovedBuild {

                /**
                 * Constructs a new ApplyApprovedBuild.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ApplyApprovedBuild.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ApplyApprovedBuild projectId. */
                projectId: string;

                /** ApplyApprovedBuild approvedBuildJson. */
                approvedBuildJson: Uint8Array;

                /** ApplyApprovedBuild runId. */
                runId: string;

                /** ApplyApprovedBuild taskId. */
                taskId: string;

                /**
                 * Encodes the specified ApplyApprovedBuild message. Does not implicitly {@link evohime.desktop.v1.ApplyApprovedBuild.verify|verify} messages.
                 * @param message ApplyApprovedBuild message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ApplyApprovedBuild.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an ApplyApprovedBuild message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ApplyApprovedBuild & evohime.desktop.v1.ApplyApprovedBuild.$Shape} ApplyApprovedBuild
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ApplyApprovedBuild & evohime.desktop.v1.ApplyApprovedBuild.$Shape;

                /**
                 * Gets the type url for ApplyApprovedBuild
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ApplyApprovedBuild {

                /** Properties of an ApplyApprovedBuild. */
                interface $Properties {

                    /** ApplyApprovedBuild projectId */
                    projectId?: (string|null);

                    /** ApplyApprovedBuild approvedBuildJson */
                    approvedBuildJson?: (Uint8Array|null);

                    /** ApplyApprovedBuild runId */
                    runId?: (string|null);

                    /** ApplyApprovedBuild taskId */
                    taskId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an ApplyApprovedBuild. */
                type $Shape = evohime.desktop.v1.ApplyApprovedBuild.$Properties;
            }

            /**
             * Properties of a PrepareBuild.
             * @deprecated Use evohime.desktop.v1.PrepareBuild.$Properties instead.
             */
            interface IPrepareBuild extends evohime.desktop.v1.PrepareBuild.$Properties {
            }

            /** Represents a PrepareBuild. */
            class PrepareBuild {

                /**
                 * Constructs a new PrepareBuild.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.PrepareBuild.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** PrepareBuild projectId. */
                projectId: string;

                /** PrepareBuild proposalJson. */
                proposalJson: Uint8Array;

                /**
                 * Encodes the specified PrepareBuild message. Does not implicitly {@link evohime.desktop.v1.PrepareBuild.verify|verify} messages.
                 * @param message PrepareBuild message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.PrepareBuild.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a PrepareBuild message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.PrepareBuild & evohime.desktop.v1.PrepareBuild.$Shape} PrepareBuild
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.PrepareBuild & evohime.desktop.v1.PrepareBuild.$Shape;

                /**
                 * Gets the type url for PrepareBuild
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace PrepareBuild {

                /** Properties of a PrepareBuild. */
                interface $Properties {

                    /** PrepareBuild projectId */
                    projectId?: (string|null);

                    /** PrepareBuild proposalJson */
                    proposalJson?: (Uint8Array|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a PrepareBuild. */
                type $Shape = evohime.desktop.v1.PrepareBuild.$Properties;
            }

            /**
             * Properties of a GetTaskSnapshot.
             * @deprecated Use evohime.desktop.v1.GetTaskSnapshot.$Properties instead.
             */
            interface IGetTaskSnapshot extends evohime.desktop.v1.GetTaskSnapshot.$Properties {
            }

            /** Represents a GetTaskSnapshot. */
            class GetTaskSnapshot {

                /**
                 * Constructs a new GetTaskSnapshot.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetTaskSnapshot.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GetTaskSnapshot projectId. */
                projectId: string;

                /** GetTaskSnapshot taskId. */
                taskId: string;

                /**
                 * Encodes the specified GetTaskSnapshot message. Does not implicitly {@link evohime.desktop.v1.GetTaskSnapshot.verify|verify} messages.
                 * @param message GetTaskSnapshot message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetTaskSnapshot.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetTaskSnapshot message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetTaskSnapshot & evohime.desktop.v1.GetTaskSnapshot.$Shape} GetTaskSnapshot
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetTaskSnapshot & evohime.desktop.v1.GetTaskSnapshot.$Shape;

                /**
                 * Gets the type url for GetTaskSnapshot
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetTaskSnapshot {

                /** Properties of a GetTaskSnapshot. */
                interface $Properties {

                    /** GetTaskSnapshot projectId */
                    projectId?: (string|null);

                    /** GetTaskSnapshot taskId */
                    taskId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetTaskSnapshot. */
                type $Shape = evohime.desktop.v1.GetTaskSnapshot.$Properties;
            }

            /**
             * Properties of a RestoreTaskSnapshot.
             * @deprecated Use evohime.desktop.v1.RestoreTaskSnapshot.$Properties instead.
             */
            interface IRestoreTaskSnapshot extends evohime.desktop.v1.RestoreTaskSnapshot.$Properties {
            }

            /** Represents a RestoreTaskSnapshot. */
            class RestoreTaskSnapshot {

                /**
                 * Constructs a new RestoreTaskSnapshot.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.RestoreTaskSnapshot.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** RestoreTaskSnapshot projectId. */
                projectId: string;

                /** RestoreTaskSnapshot taskId. */
                taskId: string;

                /** RestoreTaskSnapshot snapshotId. */
                snapshotId: string;

                /**
                 * Encodes the specified RestoreTaskSnapshot message. Does not implicitly {@link evohime.desktop.v1.RestoreTaskSnapshot.verify|verify} messages.
                 * @param message RestoreTaskSnapshot message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.RestoreTaskSnapshot.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a RestoreTaskSnapshot message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RestoreTaskSnapshot & evohime.desktop.v1.RestoreTaskSnapshot.$Shape} RestoreTaskSnapshot
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.RestoreTaskSnapshot & evohime.desktop.v1.RestoreTaskSnapshot.$Shape;

                /**
                 * Gets the type url for RestoreTaskSnapshot
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace RestoreTaskSnapshot {

                /** Properties of a RestoreTaskSnapshot. */
                interface $Properties {

                    /** RestoreTaskSnapshot projectId */
                    projectId?: (string|null);

                    /** RestoreTaskSnapshot taskId */
                    taskId?: (string|null);

                    /** RestoreTaskSnapshot snapshotId */
                    snapshotId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a RestoreTaskSnapshot. */
                type $Shape = evohime.desktop.v1.RestoreTaskSnapshot.$Properties;
            }

            /**
             * Properties of a GetBuildPolicy.
             * @deprecated Use evohime.desktop.v1.GetBuildPolicy.$Properties instead.
             */
            interface IGetBuildPolicy extends evohime.desktop.v1.GetBuildPolicy.$Properties {
            }

            /** Represents a GetBuildPolicy. */
            class GetBuildPolicy {

                /**
                 * Constructs a new GetBuildPolicy.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetBuildPolicy.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GetBuildPolicy projectId. */
                projectId: string;

                /**
                 * Encodes the specified GetBuildPolicy message. Does not implicitly {@link evohime.desktop.v1.GetBuildPolicy.verify|verify} messages.
                 * @param message GetBuildPolicy message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetBuildPolicy.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetBuildPolicy message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetBuildPolicy & evohime.desktop.v1.GetBuildPolicy.$Shape} GetBuildPolicy
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetBuildPolicy & evohime.desktop.v1.GetBuildPolicy.$Shape;

                /**
                 * Gets the type url for GetBuildPolicy
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetBuildPolicy {

                /** Properties of a GetBuildPolicy. */
                interface $Properties {

                    /** GetBuildPolicy projectId */
                    projectId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetBuildPolicy. */
                type $Shape = evohime.desktop.v1.GetBuildPolicy.$Properties;
            }

            /**
             * Properties of a SaveBuildPolicy.
             * @deprecated Use evohime.desktop.v1.SaveBuildPolicy.$Properties instead.
             */
            interface ISaveBuildPolicy extends evohime.desktop.v1.SaveBuildPolicy.$Properties {
            }

            /** Represents a SaveBuildPolicy. */
            class SaveBuildPolicy {

                /**
                 * Constructs a new SaveBuildPolicy.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SaveBuildPolicy.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SaveBuildPolicy projectId. */
                projectId: string;

                /** SaveBuildPolicy policyJson. */
                policyJson: Uint8Array;

                /** SaveBuildPolicy expectedVersion. */
                expectedVersion: number;

                /**
                 * Encodes the specified SaveBuildPolicy message. Does not implicitly {@link evohime.desktop.v1.SaveBuildPolicy.verify|verify} messages.
                 * @param message SaveBuildPolicy message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SaveBuildPolicy.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SaveBuildPolicy message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SaveBuildPolicy & evohime.desktop.v1.SaveBuildPolicy.$Shape} SaveBuildPolicy
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SaveBuildPolicy & evohime.desktop.v1.SaveBuildPolicy.$Shape;

                /**
                 * Gets the type url for SaveBuildPolicy
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SaveBuildPolicy {

                /** Properties of a SaveBuildPolicy. */
                interface $Properties {

                    /** SaveBuildPolicy projectId */
                    projectId?: (string|null);

                    /** SaveBuildPolicy policyJson */
                    policyJson?: (Uint8Array|null);

                    /** SaveBuildPolicy expectedVersion */
                    expectedVersion?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SaveBuildPolicy. */
                type $Shape = evohime.desktop.v1.SaveBuildPolicy.$Properties;
            }

            /**
             * Properties of a StartTask.
             * @deprecated Use evohime.desktop.v1.StartTask.$Properties instead.
             */
            interface IStartTask extends evohime.desktop.v1.StartTask.$Properties {
            }

            /** Represents a StartTask. */
            class StartTask {

                /**
                 * Constructs a new StartTask.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.StartTask.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** StartTask taskId. */
                taskId: string;

                /** StartTask prompt. */
                prompt: string;

                /** StartTask workspacePath. */
                workspacePath: string;

                /** StartTask preferredRouteHint. */
                preferredRouteHint: string;

                /**
                 * Encodes the specified StartTask message. Does not implicitly {@link evohime.desktop.v1.StartTask.verify|verify} messages.
                 * @param message StartTask message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.StartTask.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a StartTask message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.StartTask & evohime.desktop.v1.StartTask.$Shape} StartTask
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.StartTask & evohime.desktop.v1.StartTask.$Shape;

                /**
                 * Gets the type url for StartTask
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace StartTask {

                /** Properties of a StartTask. */
                interface $Properties {

                    /** StartTask taskId */
                    taskId?: (string|null);

                    /** StartTask prompt */
                    prompt?: (string|null);

                    /** StartTask workspacePath */
                    workspacePath?: (string|null);

                    /** StartTask preferredRouteHint */
                    preferredRouteHint?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a StartTask. */
                type $Shape = evohime.desktop.v1.StartTask.$Properties;
            }

            /**
             * Properties of a ListWorkspace.
             * @deprecated Use evohime.desktop.v1.ListWorkspace.$Properties instead.
             */
            interface IListWorkspace extends evohime.desktop.v1.ListWorkspace.$Properties {
            }

            /** Represents a ListWorkspace. */
            class ListWorkspace {

                /**
                 * Constructs a new ListWorkspace.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ListWorkspace.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ListWorkspace workspacePath. */
                workspacePath: string;

                /** ListWorkspace relativePath. */
                relativePath: string;

                /** ListWorkspace maxEntries. */
                maxEntries: number;

                /**
                 * Encodes the specified ListWorkspace message. Does not implicitly {@link evohime.desktop.v1.ListWorkspace.verify|verify} messages.
                 * @param message ListWorkspace message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ListWorkspace.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ListWorkspace message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListWorkspace & evohime.desktop.v1.ListWorkspace.$Shape} ListWorkspace
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ListWorkspace & evohime.desktop.v1.ListWorkspace.$Shape;

                /**
                 * Gets the type url for ListWorkspace
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ListWorkspace {

                /** Properties of a ListWorkspace. */
                interface $Properties {

                    /** ListWorkspace workspacePath */
                    workspacePath?: (string|null);

                    /** ListWorkspace relativePath */
                    relativePath?: (string|null);

                    /** ListWorkspace maxEntries */
                    maxEntries?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ListWorkspace. */
                type $Shape = evohime.desktop.v1.ListWorkspace.$Properties;
            }

            /**
             * Properties of a ReadWorkspaceFile.
             * @deprecated Use evohime.desktop.v1.ReadWorkspaceFile.$Properties instead.
             */
            interface IReadWorkspaceFile extends evohime.desktop.v1.ReadWorkspaceFile.$Properties {
            }

            /** Represents a ReadWorkspaceFile. */
            class ReadWorkspaceFile {

                /**
                 * Constructs a new ReadWorkspaceFile.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ReadWorkspaceFile.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ReadWorkspaceFile workspacePath. */
                workspacePath: string;

                /** ReadWorkspaceFile relativePath. */
                relativePath: string;

                /** ReadWorkspaceFile maxBytes. */
                maxBytes: number;

                /**
                 * Encodes the specified ReadWorkspaceFile message. Does not implicitly {@link evohime.desktop.v1.ReadWorkspaceFile.verify|verify} messages.
                 * @param message ReadWorkspaceFile message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ReadWorkspaceFile.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ReadWorkspaceFile message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ReadWorkspaceFile & evohime.desktop.v1.ReadWorkspaceFile.$Shape} ReadWorkspaceFile
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ReadWorkspaceFile & evohime.desktop.v1.ReadWorkspaceFile.$Shape;

                /**
                 * Gets the type url for ReadWorkspaceFile
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ReadWorkspaceFile {

                /** Properties of a ReadWorkspaceFile. */
                interface $Properties {

                    /** ReadWorkspaceFile workspacePath */
                    workspacePath?: (string|null);

                    /** ReadWorkspaceFile relativePath */
                    relativePath?: (string|null);

                    /** ReadWorkspaceFile maxBytes */
                    maxBytes?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ReadWorkspaceFile. */
                type $Shape = evohime.desktop.v1.ReadWorkspaceFile.$Properties;
            }

            /**
             * Properties of a GitStatus.
             * @deprecated Use evohime.desktop.v1.GitStatus.$Properties instead.
             */
            interface IGitStatus extends evohime.desktop.v1.GitStatus.$Properties {
            }

            /** Represents a GitStatus. */
            class GitStatus {

                /**
                 * Constructs a new GitStatus.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GitStatus.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GitStatus workspacePath. */
                workspacePath: string;

                /** GitStatus maxBytes. */
                maxBytes: number;

                /**
                 * Encodes the specified GitStatus message. Does not implicitly {@link evohime.desktop.v1.GitStatus.verify|verify} messages.
                 * @param message GitStatus message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GitStatus.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GitStatus message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GitStatus & evohime.desktop.v1.GitStatus.$Shape} GitStatus
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GitStatus & evohime.desktop.v1.GitStatus.$Shape;

                /**
                 * Gets the type url for GitStatus
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GitStatus {

                /** Properties of a GitStatus. */
                interface $Properties {

                    /** GitStatus workspacePath */
                    workspacePath?: (string|null);

                    /** GitStatus maxBytes */
                    maxBytes?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GitStatus. */
                type $Shape = evohime.desktop.v1.GitStatus.$Properties;
            }

            /**
             * Properties of a GitDiff.
             * @deprecated Use evohime.desktop.v1.GitDiff.$Properties instead.
             */
            interface IGitDiff extends evohime.desktop.v1.GitDiff.$Properties {
            }

            /** Represents a GitDiff. */
            class GitDiff {

                /**
                 * Constructs a new GitDiff.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GitDiff.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GitDiff workspacePath. */
                workspacePath: string;

                /** GitDiff relativePath. */
                relativePath: string;

                /** GitDiff maxBytes. */
                maxBytes: number;

                /**
                 * Encodes the specified GitDiff message. Does not implicitly {@link evohime.desktop.v1.GitDiff.verify|verify} messages.
                 * @param message GitDiff message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GitDiff.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GitDiff message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GitDiff & evohime.desktop.v1.GitDiff.$Shape} GitDiff
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GitDiff & evohime.desktop.v1.GitDiff.$Shape;

                /**
                 * Gets the type url for GitDiff
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GitDiff {

                /** Properties of a GitDiff. */
                interface $Properties {

                    /** GitDiff workspacePath */
                    workspacePath?: (string|null);

                    /** GitDiff relativePath */
                    relativePath?: (string|null);

                    /** GitDiff maxBytes */
                    maxBytes?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GitDiff. */
                type $Shape = evohime.desktop.v1.GitDiff.$Properties;
            }

            /**
             * Properties of a TerminalExecute.
             * @deprecated Use evohime.desktop.v1.TerminalExecute.$Properties instead.
             */
            interface ITerminalExecute extends evohime.desktop.v1.TerminalExecute.$Properties {
            }

            /** Represents a TerminalExecute. */
            class TerminalExecute {

                /**
                 * Constructs a new TerminalExecute.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.TerminalExecute.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** TerminalExecute taskId. */
                taskId: string;

                /** TerminalExecute workspacePath. */
                workspacePath: string;

                /** TerminalExecute program. */
                program: string;

                /** TerminalExecute args. */
                args: string[];

                /** TerminalExecute cwd. */
                cwd: string;

                /** TerminalExecute timeoutMs. */
                timeoutMs: number;

                /** TerminalExecute approvalId. */
                approvalId: string;

                /**
                 * Encodes the specified TerminalExecute message. Does not implicitly {@link evohime.desktop.v1.TerminalExecute.verify|verify} messages.
                 * @param message TerminalExecute message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.TerminalExecute.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a TerminalExecute message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.TerminalExecute & evohime.desktop.v1.TerminalExecute.$Shape} TerminalExecute
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.TerminalExecute & evohime.desktop.v1.TerminalExecute.$Shape;

                /**
                 * Gets the type url for TerminalExecute
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace TerminalExecute {

                /** Properties of a TerminalExecute. */
                interface $Properties {

                    /** TerminalExecute taskId */
                    taskId?: (string|null);

                    /** TerminalExecute workspacePath */
                    workspacePath?: (string|null);

                    /** TerminalExecute program */
                    program?: (string|null);

                    /** TerminalExecute args */
                    args?: (string[]|null);

                    /** TerminalExecute cwd */
                    cwd?: (string|null);

                    /** TerminalExecute timeoutMs */
                    timeoutMs?: (number|null);

                    /** TerminalExecute approvalId */
                    approvalId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a TerminalExecute. */
                type $Shape = evohime.desktop.v1.TerminalExecute.$Properties;
            }

            /**
             * Properties of a StopTask.
             * @deprecated Use evohime.desktop.v1.StopTask.$Properties instead.
             */
            interface IStopTask extends evohime.desktop.v1.StopTask.$Properties {
            }

            /** Represents a StopTask. */
            class StopTask {

                /**
                 * Constructs a new StopTask.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.StopTask.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** StopTask taskId. */
                taskId: string;

                /**
                 * Encodes the specified StopTask message. Does not implicitly {@link evohime.desktop.v1.StopTask.verify|verify} messages.
                 * @param message StopTask message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.StopTask.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a StopTask message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.StopTask & evohime.desktop.v1.StopTask.$Shape} StopTask
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.StopTask & evohime.desktop.v1.StopTask.$Shape;

                /**
                 * Gets the type url for StopTask
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace StopTask {

                /** Properties of a StopTask. */
                interface $Properties {

                    /** StopTask taskId */
                    taskId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a StopTask. */
                type $Shape = evohime.desktop.v1.StopTask.$Properties;
            }

            /**
             * Properties of a ResolveApproval.
             * @deprecated Use evohime.desktop.v1.ResolveApproval.$Properties instead.
             */
            interface IResolveApproval extends evohime.desktop.v1.ResolveApproval.$Properties {
            }

            /** Represents a ResolveApproval. */
            class ResolveApproval {

                /**
                 * Constructs a new ResolveApproval.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ResolveApproval.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ResolveApproval approvalId. */
                approvalId: string;

                /** ResolveApproval granted. */
                granted: boolean;

                /**
                 * Encodes the specified ResolveApproval message. Does not implicitly {@link evohime.desktop.v1.ResolveApproval.verify|verify} messages.
                 * @param message ResolveApproval message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ResolveApproval.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ResolveApproval message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ResolveApproval & evohime.desktop.v1.ResolveApproval.$Shape} ResolveApproval
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ResolveApproval & evohime.desktop.v1.ResolveApproval.$Shape;

                /**
                 * Gets the type url for ResolveApproval
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ResolveApproval {

                /** Properties of a ResolveApproval. */
                interface $Properties {

                    /** ResolveApproval approvalId */
                    approvalId?: (string|null);

                    /** ResolveApproval granted */
                    granted?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ResolveApproval. */
                type $Shape = evohime.desktop.v1.ResolveApproval.$Properties;
            }

            /**
             * Properties of a ResolveRoutingDecision.
             * @deprecated Use evohime.desktop.v1.ResolveRoutingDecision.$Properties instead.
             */
            interface IResolveRoutingDecision extends evohime.desktop.v1.ResolveRoutingDecision.$Properties {
            }

            /** Represents a ResolveRoutingDecision. */
            class ResolveRoutingDecision {

                /**
                 * Constructs a new ResolveRoutingDecision.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ResolveRoutingDecision.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ResolveRoutingDecision traceId. */
                traceId: string;

                /** ResolveRoutingDecision approve. */
                approve: boolean;

                /**
                 * Encodes the specified ResolveRoutingDecision message. Does not implicitly {@link evohime.desktop.v1.ResolveRoutingDecision.verify|verify} messages.
                 * @param message ResolveRoutingDecision message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ResolveRoutingDecision.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ResolveRoutingDecision message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ResolveRoutingDecision & evohime.desktop.v1.ResolveRoutingDecision.$Shape} ResolveRoutingDecision
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ResolveRoutingDecision & evohime.desktop.v1.ResolveRoutingDecision.$Shape;

                /**
                 * Gets the type url for ResolveRoutingDecision
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ResolveRoutingDecision {

                /** Properties of a ResolveRoutingDecision. */
                interface $Properties {

                    /** ResolveRoutingDecision traceId */
                    traceId?: (string|null);

                    /** ResolveRoutingDecision approve */
                    approve?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ResolveRoutingDecision. */
                type $Shape = evohime.desktop.v1.ResolveRoutingDecision.$Properties;
            }

            /**
             * Properties of a RotateReceiptKey.
             * @deprecated Use evohime.desktop.v1.RotateReceiptKey.$Properties instead.
             */
            interface IRotateReceiptKey extends evohime.desktop.v1.RotateReceiptKey.$Properties {
            }

            /** Represents a RotateReceiptKey. */
            class RotateReceiptKey {

                /**
                 * Constructs a new RotateReceiptKey.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.RotateReceiptKey.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** RotateReceiptKey reason. */
                reason: string;

                /** RotateReceiptKey approvalId. */
                approvalId: string;

                /**
                 * Encodes the specified RotateReceiptKey message. Does not implicitly {@link evohime.desktop.v1.RotateReceiptKey.verify|verify} messages.
                 * @param message RotateReceiptKey message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.RotateReceiptKey.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a RotateReceiptKey message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RotateReceiptKey & evohime.desktop.v1.RotateReceiptKey.$Shape} RotateReceiptKey
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.RotateReceiptKey & evohime.desktop.v1.RotateReceiptKey.$Shape;

                /**
                 * Gets the type url for RotateReceiptKey
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace RotateReceiptKey {

                /** Properties of a RotateReceiptKey. */
                interface $Properties {

                    /** RotateReceiptKey reason */
                    reason?: (string|null);

                    /** RotateReceiptKey approvalId */
                    approvalId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a RotateReceiptKey. */
                type $Shape = evohime.desktop.v1.RotateReceiptKey.$Properties;
            }

            /**
             * Properties of a TrustReceiptGenesis.
             * @deprecated Use evohime.desktop.v1.TrustReceiptGenesis.$Properties instead.
             */
            interface ITrustReceiptGenesis extends evohime.desktop.v1.TrustReceiptGenesis.$Properties {
            }

            /** Represents a TrustReceiptGenesis. */
            class TrustReceiptGenesis {

                /**
                 * Constructs a new TrustReceiptGenesis.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.TrustReceiptGenesis.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** TrustReceiptGenesis genesisKeyId. */
                genesisKeyId: string;

                /** TrustReceiptGenesis approvalId. */
                approvalId: string;

                /** TrustReceiptGenesis source. */
                source: string;

                /**
                 * Encodes the specified TrustReceiptGenesis message. Does not implicitly {@link evohime.desktop.v1.TrustReceiptGenesis.verify|verify} messages.
                 * @param message TrustReceiptGenesis message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.TrustReceiptGenesis.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a TrustReceiptGenesis message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.TrustReceiptGenesis & evohime.desktop.v1.TrustReceiptGenesis.$Shape} TrustReceiptGenesis
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.TrustReceiptGenesis & evohime.desktop.v1.TrustReceiptGenesis.$Shape;

                /**
                 * Gets the type url for TrustReceiptGenesis
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace TrustReceiptGenesis {

                /** Properties of a TrustReceiptGenesis. */
                interface $Properties {

                    /** TrustReceiptGenesis genesisKeyId */
                    genesisKeyId?: (string|null);

                    /** TrustReceiptGenesis approvalId */
                    approvalId?: (string|null);

                    /** TrustReceiptGenesis source */
                    source?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a TrustReceiptGenesis. */
                type $Shape = evohime.desktop.v1.TrustReceiptGenesis.$Properties;
            }

            /**
             * Properties of a GetReceiptKeyStatus.
             * @deprecated Use evohime.desktop.v1.GetReceiptKeyStatus.$Properties instead.
             */
            interface IGetReceiptKeyStatus extends evohime.desktop.v1.GetReceiptKeyStatus.$Properties {
            }

            /** Represents a GetReceiptKeyStatus. */
            class GetReceiptKeyStatus {

                /**
                 * Constructs a new GetReceiptKeyStatus.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetReceiptKeyStatus.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /**
                 * Encodes the specified GetReceiptKeyStatus message. Does not implicitly {@link evohime.desktop.v1.GetReceiptKeyStatus.verify|verify} messages.
                 * @param message GetReceiptKeyStatus message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetReceiptKeyStatus.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetReceiptKeyStatus message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetReceiptKeyStatus & evohime.desktop.v1.GetReceiptKeyStatus.$Shape} GetReceiptKeyStatus
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetReceiptKeyStatus & evohime.desktop.v1.GetReceiptKeyStatus.$Shape;

                /**
                 * Gets the type url for GetReceiptKeyStatus
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetReceiptKeyStatus {

                /** Properties of a GetReceiptKeyStatus. */
                interface $Properties {

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetReceiptKeyStatus. */
                type $Shape = evohime.desktop.v1.GetReceiptKeyStatus.$Properties;
            }

            /**
             * Properties of a CreateNewReceiptGenesis.
             * @deprecated Use evohime.desktop.v1.CreateNewReceiptGenesis.$Properties instead.
             */
            interface ICreateNewReceiptGenesis extends evohime.desktop.v1.CreateNewReceiptGenesis.$Properties {
            }

            /** Represents a CreateNewReceiptGenesis. */
            class CreateNewReceiptGenesis {

                /**
                 * Constructs a new CreateNewReceiptGenesis.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.CreateNewReceiptGenesis.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** CreateNewReceiptGenesis approvalId. */
                approvalId: string;

                /** CreateNewReceiptGenesis source. */
                source: string;

                /**
                 * Encodes the specified CreateNewReceiptGenesis message. Does not implicitly {@link evohime.desktop.v1.CreateNewReceiptGenesis.verify|verify} messages.
                 * @param message CreateNewReceiptGenesis message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.CreateNewReceiptGenesis.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CreateNewReceiptGenesis message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.CreateNewReceiptGenesis & evohime.desktop.v1.CreateNewReceiptGenesis.$Shape} CreateNewReceiptGenesis
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.CreateNewReceiptGenesis & evohime.desktop.v1.CreateNewReceiptGenesis.$Shape;

                /**
                 * Gets the type url for CreateNewReceiptGenesis
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace CreateNewReceiptGenesis {

                /** Properties of a CreateNewReceiptGenesis. */
                interface $Properties {

                    /** CreateNewReceiptGenesis approvalId */
                    approvalId?: (string|null);

                    /** CreateNewReceiptGenesis source */
                    source?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a CreateNewReceiptGenesis. */
                type $Shape = evohime.desktop.v1.CreateNewReceiptGenesis.$Properties;
            }

            /**
             * Properties of a ClosePendingReceiptAction.
             * @deprecated Use evohime.desktop.v1.ClosePendingReceiptAction.$Properties instead.
             */
            interface IClosePendingReceiptAction extends evohime.desktop.v1.ClosePendingReceiptAction.$Properties {
            }

            /** Represents a ClosePendingReceiptAction. */
            class ClosePendingReceiptAction {

                /**
                 * Constructs a new ClosePendingReceiptAction.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ClosePendingReceiptAction.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ClosePendingReceiptAction actionId. */
                actionId: string;

                /** ClosePendingReceiptAction inputJson. */
                inputJson: string;

                /** ClosePendingReceiptAction operatorConfirmed. */
                operatorConfirmed: boolean;

                /**
                 * Encodes the specified ClosePendingReceiptAction message. Does not implicitly {@link evohime.desktop.v1.ClosePendingReceiptAction.verify|verify} messages.
                 * @param message ClosePendingReceiptAction message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ClosePendingReceiptAction.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ClosePendingReceiptAction message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ClosePendingReceiptAction & evohime.desktop.v1.ClosePendingReceiptAction.$Shape} ClosePendingReceiptAction
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ClosePendingReceiptAction & evohime.desktop.v1.ClosePendingReceiptAction.$Shape;

                /**
                 * Gets the type url for ClosePendingReceiptAction
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ClosePendingReceiptAction {

                /** Properties of a ClosePendingReceiptAction. */
                interface $Properties {

                    /** ClosePendingReceiptAction actionId */
                    actionId?: (string|null);

                    /** ClosePendingReceiptAction inputJson */
                    inputJson?: (string|null);

                    /** ClosePendingReceiptAction operatorConfirmed */
                    operatorConfirmed?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ClosePendingReceiptAction. */
                type $Shape = evohime.desktop.v1.ClosePendingReceiptAction.$Properties;
            }

            /**
             * Properties of a SetReceiptAuditSamplingRate.
             * @deprecated Use evohime.desktop.v1.SetReceiptAuditSamplingRate.$Properties instead.
             */
            interface ISetReceiptAuditSamplingRate extends evohime.desktop.v1.SetReceiptAuditSamplingRate.$Properties {
            }

            /** Represents a SetReceiptAuditSamplingRate. */
            class SetReceiptAuditSamplingRate {

                /**
                 * Constructs a new SetReceiptAuditSamplingRate.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SetReceiptAuditSamplingRate.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SetReceiptAuditSamplingRate rate. */
                rate: number;

                /**
                 * Encodes the specified SetReceiptAuditSamplingRate message. Does not implicitly {@link evohime.desktop.v1.SetReceiptAuditSamplingRate.verify|verify} messages.
                 * @param message SetReceiptAuditSamplingRate message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SetReceiptAuditSamplingRate.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SetReceiptAuditSamplingRate message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SetReceiptAuditSamplingRate & evohime.desktop.v1.SetReceiptAuditSamplingRate.$Shape} SetReceiptAuditSamplingRate
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SetReceiptAuditSamplingRate & evohime.desktop.v1.SetReceiptAuditSamplingRate.$Shape;

                /**
                 * Gets the type url for SetReceiptAuditSamplingRate
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SetReceiptAuditSamplingRate {

                /** Properties of a SetReceiptAuditSamplingRate. */
                interface $Properties {

                    /** SetReceiptAuditSamplingRate rate */
                    rate?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SetReceiptAuditSamplingRate. */
                type $Shape = evohime.desktop.v1.SetReceiptAuditSamplingRate.$Properties;
            }

            /**
             * Properties of a ReconcilePendingReceiptAction.
             * @deprecated Use evohime.desktop.v1.ReconcilePendingReceiptAction.$Properties instead.
             */
            interface IReconcilePendingReceiptAction extends evohime.desktop.v1.ReconcilePendingReceiptAction.$Properties {
            }

            /** Represents a ReconcilePendingReceiptAction. */
            class ReconcilePendingReceiptAction {

                /**
                 * Constructs a new ReconcilePendingReceiptAction.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ReconcilePendingReceiptAction.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ReconcilePendingReceiptAction oldActionId. */
                oldActionId: string;

                /** ReconcilePendingReceiptAction toolName. */
                toolName: string;

                /** ReconcilePendingReceiptAction inputJson. */
                inputJson: string;

                /** ReconcilePendingReceiptAction workspacePath. */
                workspacePath: string;

                /**
                 * Encodes the specified ReconcilePendingReceiptAction message. Does not implicitly {@link evohime.desktop.v1.ReconcilePendingReceiptAction.verify|verify} messages.
                 * @param message ReconcilePendingReceiptAction message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ReconcilePendingReceiptAction.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ReconcilePendingReceiptAction message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ReconcilePendingReceiptAction & evohime.desktop.v1.ReconcilePendingReceiptAction.$Shape} ReconcilePendingReceiptAction
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ReconcilePendingReceiptAction & evohime.desktop.v1.ReconcilePendingReceiptAction.$Shape;

                /**
                 * Gets the type url for ReconcilePendingReceiptAction
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ReconcilePendingReceiptAction {

                /** Properties of a ReconcilePendingReceiptAction. */
                interface $Properties {

                    /** ReconcilePendingReceiptAction oldActionId */
                    oldActionId?: (string|null);

                    /** ReconcilePendingReceiptAction toolName */
                    toolName?: (string|null);

                    /** ReconcilePendingReceiptAction inputJson */
                    inputJson?: (string|null);

                    /** ReconcilePendingReceiptAction workspacePath */
                    workspacePath?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ReconcilePendingReceiptAction. */
                type $Shape = evohime.desktop.v1.ReconcilePendingReceiptAction.$Properties;
            }

            /**
             * Properties of an UnquarantineReceiptAction.
             * @deprecated Use evohime.desktop.v1.UnquarantineReceiptAction.$Properties instead.
             */
            interface IUnquarantineReceiptAction extends evohime.desktop.v1.UnquarantineReceiptAction.$Properties {
            }

            /** Represents an UnquarantineReceiptAction. */
            class UnquarantineReceiptAction {

                /**
                 * Constructs a new UnquarantineReceiptAction.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.UnquarantineReceiptAction.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** UnquarantineReceiptAction actionId. */
                actionId: string;

                /** UnquarantineReceiptAction inputJson. */
                inputJson: string;

                /** UnquarantineReceiptAction operatorConfirmed. */
                operatorConfirmed: boolean;

                /** UnquarantineReceiptAction checkpoint. */
                checkpoint: string;

                /**
                 * Encodes the specified UnquarantineReceiptAction message. Does not implicitly {@link evohime.desktop.v1.UnquarantineReceiptAction.verify|verify} messages.
                 * @param message UnquarantineReceiptAction message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.UnquarantineReceiptAction.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an UnquarantineReceiptAction message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.UnquarantineReceiptAction & evohime.desktop.v1.UnquarantineReceiptAction.$Shape} UnquarantineReceiptAction
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.UnquarantineReceiptAction & evohime.desktop.v1.UnquarantineReceiptAction.$Shape;

                /**
                 * Gets the type url for UnquarantineReceiptAction
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace UnquarantineReceiptAction {

                /** Properties of an UnquarantineReceiptAction. */
                interface $Properties {

                    /** UnquarantineReceiptAction actionId */
                    actionId?: (string|null);

                    /** UnquarantineReceiptAction inputJson */
                    inputJson?: (string|null);

                    /** UnquarantineReceiptAction operatorConfirmed */
                    operatorConfirmed?: (boolean|null);

                    /** UnquarantineReceiptAction checkpoint */
                    checkpoint?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an UnquarantineReceiptAction. */
                type $Shape = evohime.desktop.v1.UnquarantineReceiptAction.$Properties;
            }

            /**
             * Properties of a ListReceipts.
             * @deprecated Use evohime.desktop.v1.ListReceipts.$Properties instead.
             */
            interface IListReceipts extends evohime.desktop.v1.ListReceipts.$Properties {
            }

            /** Represents a ListReceipts. */
            class ListReceipts {

                /**
                 * Constructs a new ListReceipts.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ListReceipts.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ListReceipts taskId. */
                taskId: string;

                /** ListReceipts runId. */
                runId: string;

                /** ListReceipts actionId. */
                actionId: string;

                /** ListReceipts fromRfc3339. */
                fromRfc3339: string;

                /** ListReceipts toRfc3339. */
                toRfc3339: string;

                /** ListReceipts limit. */
                limit: number;

                /**
                 * Encodes the specified ListReceipts message. Does not implicitly {@link evohime.desktop.v1.ListReceipts.verify|verify} messages.
                 * @param message ListReceipts message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ListReceipts.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ListReceipts message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListReceipts & evohime.desktop.v1.ListReceipts.$Shape} ListReceipts
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ListReceipts & evohime.desktop.v1.ListReceipts.$Shape;

                /**
                 * Gets the type url for ListReceipts
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ListReceipts {

                /** Properties of a ListReceipts. */
                interface $Properties {

                    /** ListReceipts taskId */
                    taskId?: (string|null);

                    /** ListReceipts runId */
                    runId?: (string|null);

                    /** ListReceipts actionId */
                    actionId?: (string|null);

                    /** ListReceipts fromRfc3339 */
                    fromRfc3339?: (string|null);

                    /** ListReceipts toRfc3339 */
                    toRfc3339?: (string|null);

                    /** ListReceipts limit */
                    limit?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ListReceipts. */
                type $Shape = evohime.desktop.v1.ListReceipts.$Properties;
            }

            /**
             * Properties of a VerifyReceipts.
             * @deprecated Use evohime.desktop.v1.VerifyReceipts.$Properties instead.
             */
            interface IVerifyReceipts extends evohime.desktop.v1.VerifyReceipts.$Properties {
            }

            /** Represents a VerifyReceipts. */
            class VerifyReceipts {

                /**
                 * Constructs a new VerifyReceipts.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.VerifyReceipts.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** VerifyReceipts taskId. */
                taskId: string;

                /** VerifyReceipts runId. */
                runId: string;

                /** VerifyReceipts actionId. */
                actionId: string;

                /** VerifyReceipts fromRfc3339. */
                fromRfc3339: string;

                /** VerifyReceipts toRfc3339. */
                toRfc3339: string;

                /** VerifyReceipts limit. */
                limit: number;

                /** VerifyReceipts trustKeyId. */
                trustKeyId: string;

                /**
                 * Encodes the specified VerifyReceipts message. Does not implicitly {@link evohime.desktop.v1.VerifyReceipts.verify|verify} messages.
                 * @param message VerifyReceipts message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.VerifyReceipts.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a VerifyReceipts message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.VerifyReceipts & evohime.desktop.v1.VerifyReceipts.$Shape} VerifyReceipts
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.VerifyReceipts & evohime.desktop.v1.VerifyReceipts.$Shape;

                /**
                 * Gets the type url for VerifyReceipts
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace VerifyReceipts {

                /** Properties of a VerifyReceipts. */
                interface $Properties {

                    /** VerifyReceipts taskId */
                    taskId?: (string|null);

                    /** VerifyReceipts runId */
                    runId?: (string|null);

                    /** VerifyReceipts actionId */
                    actionId?: (string|null);

                    /** VerifyReceipts fromRfc3339 */
                    fromRfc3339?: (string|null);

                    /** VerifyReceipts toRfc3339 */
                    toRfc3339?: (string|null);

                    /** VerifyReceipts limit */
                    limit?: (number|null);

                    /** VerifyReceipts trustKeyId */
                    trustKeyId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a VerifyReceipts. */
                type $Shape = evohime.desktop.v1.VerifyReceipts.$Properties;
            }

            /**
             * Properties of an ExportReceipts.
             * @deprecated Use evohime.desktop.v1.ExportReceipts.$Properties instead.
             */
            interface IExportReceipts extends evohime.desktop.v1.ExportReceipts.$Properties {
            }

            /** Represents an ExportReceipts. */
            class ExportReceipts {

                /**
                 * Constructs a new ExportReceipts.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ExportReceipts.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ExportReceipts destinationPath. */
                destinationPath: string;

                /** ExportReceipts taskId. */
                taskId: string;

                /** ExportReceipts runId. */
                runId: string;

                /** ExportReceipts actionId. */
                actionId: string;

                /** ExportReceipts fromRfc3339. */
                fromRfc3339: string;

                /** ExportReceipts toRfc3339. */
                toRfc3339: string;

                /** ExportReceipts limit. */
                limit: number;

                /** ExportReceipts replace. */
                replace: boolean;

                /**
                 * Encodes the specified ExportReceipts message. Does not implicitly {@link evohime.desktop.v1.ExportReceipts.verify|verify} messages.
                 * @param message ExportReceipts message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ExportReceipts.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an ExportReceipts message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ExportReceipts & evohime.desktop.v1.ExportReceipts.$Shape} ExportReceipts
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ExportReceipts & evohime.desktop.v1.ExportReceipts.$Shape;

                /**
                 * Gets the type url for ExportReceipts
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ExportReceipts {

                /** Properties of an ExportReceipts. */
                interface $Properties {

                    /** ExportReceipts destinationPath */
                    destinationPath?: (string|null);

                    /** ExportReceipts taskId */
                    taskId?: (string|null);

                    /** ExportReceipts runId */
                    runId?: (string|null);

                    /** ExportReceipts actionId */
                    actionId?: (string|null);

                    /** ExportReceipts fromRfc3339 */
                    fromRfc3339?: (string|null);

                    /** ExportReceipts toRfc3339 */
                    toRfc3339?: (string|null);

                    /** ExportReceipts limit */
                    limit?: (number|null);

                    /** ExportReceipts replace */
                    replace?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an ExportReceipts. */
                type $Shape = evohime.desktop.v1.ExportReceipts.$Properties;
            }

            /**
             * Properties of a RunDoctor.
             * @deprecated Use evohime.desktop.v1.RunDoctor.$Properties instead.
             */
            interface IRunDoctor extends evohime.desktop.v1.RunDoctor.$Properties {
            }

            /** Represents a RunDoctor. */
            class RunDoctor {

                /**
                 * Constructs a new RunDoctor.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.RunDoctor.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** RunDoctor projectId. */
                projectId: string;

                /** RunDoctor detailLevel. */
                detailLevel: number;

                /**
                 * Encodes the specified RunDoctor message. Does not implicitly {@link evohime.desktop.v1.RunDoctor.verify|verify} messages.
                 * @param message RunDoctor message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.RunDoctor.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a RunDoctor message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RunDoctor & evohime.desktop.v1.RunDoctor.$Shape} RunDoctor
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.RunDoctor & evohime.desktop.v1.RunDoctor.$Shape;

                /**
                 * Gets the type url for RunDoctor
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace RunDoctor {

                /** Properties of a RunDoctor. */
                interface $Properties {

                    /** RunDoctor projectId */
                    projectId?: (string|null);

                    /** RunDoctor detailLevel */
                    detailLevel?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a RunDoctor. */
                type $Shape = evohime.desktop.v1.RunDoctor.$Properties;
            }

            /**
             * Properties of an ExportDoctorLogs.
             * @deprecated Use evohime.desktop.v1.ExportDoctorLogs.$Properties instead.
             */
            interface IExportDoctorLogs extends evohime.desktop.v1.ExportDoctorLogs.$Properties {
            }

            /** Represents an ExportDoctorLogs. */
            class ExportDoctorLogs {

                /**
                 * Constructs a new ExportDoctorLogs.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ExportDoctorLogs.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ExportDoctorLogs destinationPath. */
                destinationPath: string;

                /**
                 * Encodes the specified ExportDoctorLogs message. Does not implicitly {@link evohime.desktop.v1.ExportDoctorLogs.verify|verify} messages.
                 * @param message ExportDoctorLogs message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ExportDoctorLogs.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an ExportDoctorLogs message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ExportDoctorLogs & evohime.desktop.v1.ExportDoctorLogs.$Shape} ExportDoctorLogs
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ExportDoctorLogs & evohime.desktop.v1.ExportDoctorLogs.$Shape;

                /**
                 * Gets the type url for ExportDoctorLogs
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ExportDoctorLogs {

                /** Properties of an ExportDoctorLogs. */
                interface $Properties {

                    /** ExportDoctorLogs destinationPath */
                    destinationPath?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an ExportDoctorLogs. */
                type $Shape = evohime.desktop.v1.ExportDoctorLogs.$Properties;
            }

            /**
             * Properties of a CreateDatabaseBackup.
             * @deprecated Use evohime.desktop.v1.CreateDatabaseBackup.$Properties instead.
             */
            interface ICreateDatabaseBackup extends evohime.desktop.v1.CreateDatabaseBackup.$Properties {
            }

            /** Represents a CreateDatabaseBackup. */
            class CreateDatabaseBackup {

                /**
                 * Constructs a new CreateDatabaseBackup.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.CreateDatabaseBackup.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** CreateDatabaseBackup destinationPath. */
                destinationPath: string;

                /**
                 * Encodes the specified CreateDatabaseBackup message. Does not implicitly {@link evohime.desktop.v1.CreateDatabaseBackup.verify|verify} messages.
                 * @param message CreateDatabaseBackup message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.CreateDatabaseBackup.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CreateDatabaseBackup message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.CreateDatabaseBackup & evohime.desktop.v1.CreateDatabaseBackup.$Shape} CreateDatabaseBackup
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.CreateDatabaseBackup & evohime.desktop.v1.CreateDatabaseBackup.$Shape;

                /**
                 * Gets the type url for CreateDatabaseBackup
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace CreateDatabaseBackup {

                /** Properties of a CreateDatabaseBackup. */
                interface $Properties {

                    /** CreateDatabaseBackup destinationPath */
                    destinationPath?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a CreateDatabaseBackup. */
                type $Shape = evohime.desktop.v1.CreateDatabaseBackup.$Properties;
            }

            /**
             * Properties of a PrepareDatabaseRestore.
             * @deprecated Use evohime.desktop.v1.PrepareDatabaseRestore.$Properties instead.
             */
            interface IPrepareDatabaseRestore extends evohime.desktop.v1.PrepareDatabaseRestore.$Properties {
            }

            /** Represents a PrepareDatabaseRestore. */
            class PrepareDatabaseRestore {

                /**
                 * Constructs a new PrepareDatabaseRestore.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.PrepareDatabaseRestore.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** PrepareDatabaseRestore backupPath. */
                backupPath: string;

                /**
                 * Encodes the specified PrepareDatabaseRestore message. Does not implicitly {@link evohime.desktop.v1.PrepareDatabaseRestore.verify|verify} messages.
                 * @param message PrepareDatabaseRestore message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.PrepareDatabaseRestore.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a PrepareDatabaseRestore message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.PrepareDatabaseRestore & evohime.desktop.v1.PrepareDatabaseRestore.$Shape} PrepareDatabaseRestore
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.PrepareDatabaseRestore & evohime.desktop.v1.PrepareDatabaseRestore.$Shape;

                /**
                 * Gets the type url for PrepareDatabaseRestore
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace PrepareDatabaseRestore {

                /** Properties of a PrepareDatabaseRestore. */
                interface $Properties {

                    /** PrepareDatabaseRestore backupPath */
                    backupPath?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a PrepareDatabaseRestore. */
                type $Shape = evohime.desktop.v1.PrepareDatabaseRestore.$Properties;
            }

            /**
             * Properties of a RestoreDatabase.
             * @deprecated Use evohime.desktop.v1.RestoreDatabase.$Properties instead.
             */
            interface IRestoreDatabase extends evohime.desktop.v1.RestoreDatabase.$Properties {
            }

            /** Represents a RestoreDatabase. */
            class RestoreDatabase {

                /**
                 * Constructs a new RestoreDatabase.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.RestoreDatabase.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** RestoreDatabase backupPath. */
                backupPath: string;

                /** RestoreDatabase approvalId. */
                approvalId: string;

                /**
                 * Encodes the specified RestoreDatabase message. Does not implicitly {@link evohime.desktop.v1.RestoreDatabase.verify|verify} messages.
                 * @param message RestoreDatabase message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.RestoreDatabase.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a RestoreDatabase message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RestoreDatabase & evohime.desktop.v1.RestoreDatabase.$Shape} RestoreDatabase
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.RestoreDatabase & evohime.desktop.v1.RestoreDatabase.$Shape;

                /**
                 * Gets the type url for RestoreDatabase
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace RestoreDatabase {

                /** Properties of a RestoreDatabase. */
                interface $Properties {

                    /** RestoreDatabase backupPath */
                    backupPath?: (string|null);

                    /** RestoreDatabase approvalId */
                    approvalId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a RestoreDatabase. */
                type $Shape = evohime.desktop.v1.RestoreDatabase.$Properties;
            }

            /**
             * Properties of a CancelDatabaseOperation.
             * @deprecated Use evohime.desktop.v1.CancelDatabaseOperation.$Properties instead.
             */
            interface ICancelDatabaseOperation extends evohime.desktop.v1.CancelDatabaseOperation.$Properties {
            }

            /** Represents a CancelDatabaseOperation. */
            class CancelDatabaseOperation {

                /**
                 * Constructs a new CancelDatabaseOperation.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.CancelDatabaseOperation.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** CancelDatabaseOperation operationId. */
                operationId: string;

                /**
                 * Encodes the specified CancelDatabaseOperation message. Does not implicitly {@link evohime.desktop.v1.CancelDatabaseOperation.verify|verify} messages.
                 * @param message CancelDatabaseOperation message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.CancelDatabaseOperation.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CancelDatabaseOperation message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.CancelDatabaseOperation & evohime.desktop.v1.CancelDatabaseOperation.$Shape} CancelDatabaseOperation
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.CancelDatabaseOperation & evohime.desktop.v1.CancelDatabaseOperation.$Shape;

                /**
                 * Gets the type url for CancelDatabaseOperation
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace CancelDatabaseOperation {

                /** Properties of a CancelDatabaseOperation. */
                interface $Properties {

                    /** CancelDatabaseOperation operationId */
                    operationId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a CancelDatabaseOperation. */
                type $Shape = evohime.desktop.v1.CancelDatabaseOperation.$Properties;
            }

            /**
             * Properties of a SaveResearchEvidence.
             * @deprecated Use evohime.desktop.v1.SaveResearchEvidence.$Properties instead.
             */
            interface ISaveResearchEvidence extends evohime.desktop.v1.SaveResearchEvidence.$Properties {
            }

            /** Represents a SaveResearchEvidence. */
            class SaveResearchEvidence {

                /**
                 * Constructs a new SaveResearchEvidence.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SaveResearchEvidence.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SaveResearchEvidence workItemId. */
                workItemId: string;

                /** SaveResearchEvidence sourceKind. */
                sourceKind: string;

                /** SaveResearchEvidence sourceRef. */
                sourceRef: string;

                /** SaveResearchEvidence title. */
                title: string;

                /** SaveResearchEvidence publisher. */
                publisher: string;

                /** SaveResearchEvidence contentType. */
                contentType: string;

                /** SaveResearchEvidence rawExcerpt. */
                rawExcerpt: string;

                /** SaveResearchEvidence retrievedAtMs. */
                retrievedAtMs: number;

                /** SaveResearchEvidence ttlMs. */
                ttlMs: number;

                /**
                 * Encodes the specified SaveResearchEvidence message. Does not implicitly {@link evohime.desktop.v1.SaveResearchEvidence.verify|verify} messages.
                 * @param message SaveResearchEvidence message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SaveResearchEvidence.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SaveResearchEvidence message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SaveResearchEvidence & evohime.desktop.v1.SaveResearchEvidence.$Shape} SaveResearchEvidence
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SaveResearchEvidence & evohime.desktop.v1.SaveResearchEvidence.$Shape;

                /**
                 * Gets the type url for SaveResearchEvidence
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SaveResearchEvidence {

                /** Properties of a SaveResearchEvidence. */
                interface $Properties {

                    /** SaveResearchEvidence workItemId */
                    workItemId?: (string|null);

                    /** SaveResearchEvidence sourceKind */
                    sourceKind?: (string|null);

                    /** SaveResearchEvidence sourceRef */
                    sourceRef?: (string|null);

                    /** SaveResearchEvidence title */
                    title?: (string|null);

                    /** SaveResearchEvidence publisher */
                    publisher?: (string|null);

                    /** SaveResearchEvidence contentType */
                    contentType?: (string|null);

                    /** SaveResearchEvidence rawExcerpt */
                    rawExcerpt?: (string|null);

                    /** SaveResearchEvidence retrievedAtMs */
                    retrievedAtMs?: (number|null);

                    /** SaveResearchEvidence ttlMs */
                    ttlMs?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SaveResearchEvidence. */
                type $Shape = evohime.desktop.v1.SaveResearchEvidence.$Properties;
            }

            /**
             * Properties of a ListResearchEvidence.
             * @deprecated Use evohime.desktop.v1.ListResearchEvidence.$Properties instead.
             */
            interface IListResearchEvidence extends evohime.desktop.v1.ListResearchEvidence.$Properties {
            }

            /** Represents a ListResearchEvidence. */
            class ListResearchEvidence {

                /**
                 * Constructs a new ListResearchEvidence.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ListResearchEvidence.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ListResearchEvidence workItemId. */
                workItemId: string;

                /**
                 * Encodes the specified ListResearchEvidence message. Does not implicitly {@link evohime.desktop.v1.ListResearchEvidence.verify|verify} messages.
                 * @param message ListResearchEvidence message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ListResearchEvidence.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ListResearchEvidence message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListResearchEvidence & evohime.desktop.v1.ListResearchEvidence.$Shape} ListResearchEvidence
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ListResearchEvidence & evohime.desktop.v1.ListResearchEvidence.$Shape;

                /**
                 * Gets the type url for ListResearchEvidence
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ListResearchEvidence {

                /** Properties of a ListResearchEvidence. */
                interface $Properties {

                    /** ListResearchEvidence workItemId */
                    workItemId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ListResearchEvidence. */
                type $Shape = evohime.desktop.v1.ListResearchEvidence.$Properties;
            }

            /**
             * Properties of a RunResearchFetch.
             * @deprecated Use evohime.desktop.v1.RunResearchFetch.$Properties instead.
             */
            interface IRunResearchFetch extends evohime.desktop.v1.RunResearchFetch.$Properties {
            }

            /** Represents a RunResearchFetch. */
            class RunResearchFetch {

                /**
                 * Constructs a new RunResearchFetch.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.RunResearchFetch.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** RunResearchFetch workItemId. */
                workItemId: string;

                /** RunResearchFetch url. */
                url: string;

                /** RunResearchFetch title. */
                title: string;

                /** RunResearchFetch allowedDomains. */
                allowedDomains: string[];

                /** RunResearchFetch maxBytes. */
                maxBytes: number;

                /** RunResearchFetch maxLatencyMs. */
                maxLatencyMs: number;

                /** RunResearchFetch maxCostMicros. */
                maxCostMicros: number;

                /** RunResearchFetch ttlMs. */
                ttlMs: number;

                /**
                 * Encodes the specified RunResearchFetch message. Does not implicitly {@link evohime.desktop.v1.RunResearchFetch.verify|verify} messages.
                 * @param message RunResearchFetch message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.RunResearchFetch.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a RunResearchFetch message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RunResearchFetch & evohime.desktop.v1.RunResearchFetch.$Shape} RunResearchFetch
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.RunResearchFetch & evohime.desktop.v1.RunResearchFetch.$Shape;

                /**
                 * Gets the type url for RunResearchFetch
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace RunResearchFetch {

                /** Properties of a RunResearchFetch. */
                interface $Properties {

                    /** RunResearchFetch workItemId */
                    workItemId?: (string|null);

                    /** RunResearchFetch url */
                    url?: (string|null);

                    /** RunResearchFetch title */
                    title?: (string|null);

                    /** RunResearchFetch allowedDomains */
                    allowedDomains?: (string[]|null);

                    /** RunResearchFetch maxBytes */
                    maxBytes?: (number|null);

                    /** RunResearchFetch maxLatencyMs */
                    maxLatencyMs?: (number|null);

                    /** RunResearchFetch maxCostMicros */
                    maxCostMicros?: (number|null);

                    /** RunResearchFetch ttlMs */
                    ttlMs?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a RunResearchFetch. */
                type $Shape = evohime.desktop.v1.RunResearchFetch.$Properties;
            }

            /**
             * Properties of a CreateMemory.
             * @deprecated Use evohime.desktop.v1.CreateMemory.$Properties instead.
             */
            interface ICreateMemory extends evohime.desktop.v1.CreateMemory.$Properties {
            }

            /** Represents a CreateMemory. */
            class CreateMemory {

                /**
                 * Constructs a new CreateMemory.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.CreateMemory.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** CreateMemory scopeKind. */
                scopeKind: string;

                /** CreateMemory projectId. */
                projectId: string;

                /** CreateMemory secondaryId. */
                secondaryId: string;

                /** CreateMemory title. */
                title: string;

                /** CreateMemory content. */
                content: string;

                /** CreateMemory provenanceKind. */
                provenanceKind: string;

                /** CreateMemory provenanceId. */
                provenanceId: string;

                /** CreateMemory provenanceLocator. */
                provenanceLocator: string;

                /** CreateMemory privacy. */
                privacy: string;

                /** CreateMemory ttlMs. */
                ttlMs: number;

                /**
                 * Encodes the specified CreateMemory message. Does not implicitly {@link evohime.desktop.v1.CreateMemory.verify|verify} messages.
                 * @param message CreateMemory message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.CreateMemory.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CreateMemory message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.CreateMemory & evohime.desktop.v1.CreateMemory.$Shape} CreateMemory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.CreateMemory & evohime.desktop.v1.CreateMemory.$Shape;

                /**
                 * Gets the type url for CreateMemory
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace CreateMemory {

                /** Properties of a CreateMemory. */
                interface $Properties {

                    /** CreateMemory scopeKind */
                    scopeKind?: (string|null);

                    /** CreateMemory projectId */
                    projectId?: (string|null);

                    /** CreateMemory secondaryId */
                    secondaryId?: (string|null);

                    /** CreateMemory title */
                    title?: (string|null);

                    /** CreateMemory content */
                    content?: (string|null);

                    /** CreateMemory provenanceKind */
                    provenanceKind?: (string|null);

                    /** CreateMemory provenanceId */
                    provenanceId?: (string|null);

                    /** CreateMemory provenanceLocator */
                    provenanceLocator?: (string|null);

                    /** CreateMemory privacy */
                    privacy?: (string|null);

                    /** CreateMemory ttlMs */
                    ttlMs?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a CreateMemory. */
                type $Shape = evohime.desktop.v1.CreateMemory.$Properties;
            }

            /**
             * Properties of a ListMemory.
             * @deprecated Use evohime.desktop.v1.ListMemory.$Properties instead.
             */
            interface IListMemory extends evohime.desktop.v1.ListMemory.$Properties {
            }

            /** Represents a ListMemory. */
            class ListMemory {

                /**
                 * Constructs a new ListMemory.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ListMemory.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ListMemory scopeKind. */
                scopeKind: string;

                /** ListMemory projectId. */
                projectId: string;

                /** ListMemory secondaryId. */
                secondaryId: string;

                /** ListMemory includeArchived. */
                includeArchived: boolean;

                /** ListMemory limit. */
                limit: number;

                /**
                 * Encodes the specified ListMemory message. Does not implicitly {@link evohime.desktop.v1.ListMemory.verify|verify} messages.
                 * @param message ListMemory message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ListMemory.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ListMemory message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListMemory & evohime.desktop.v1.ListMemory.$Shape} ListMemory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ListMemory & evohime.desktop.v1.ListMemory.$Shape;

                /**
                 * Gets the type url for ListMemory
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ListMemory {

                /** Properties of a ListMemory. */
                interface $Properties {

                    /** ListMemory scopeKind */
                    scopeKind?: (string|null);

                    /** ListMemory projectId */
                    projectId?: (string|null);

                    /** ListMemory secondaryId */
                    secondaryId?: (string|null);

                    /** ListMemory includeArchived */
                    includeArchived?: (boolean|null);

                    /** ListMemory limit */
                    limit?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ListMemory. */
                type $Shape = evohime.desktop.v1.ListMemory.$Properties;
            }

            /**
             * Properties of a SearchMemory.
             * @deprecated Use evohime.desktop.v1.SearchMemory.$Properties instead.
             */
            interface ISearchMemory extends evohime.desktop.v1.SearchMemory.$Properties {
            }

            /** Represents a SearchMemory. */
            class SearchMemory {

                /**
                 * Constructs a new SearchMemory.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SearchMemory.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SearchMemory scopeKind. */
                scopeKind: string;

                /** SearchMemory projectId. */
                projectId: string;

                /** SearchMemory secondaryId. */
                secondaryId: string;

                /** SearchMemory query. */
                query: string;

                /** SearchMemory limit. */
                limit: number;

                /**
                 * Encodes the specified SearchMemory message. Does not implicitly {@link evohime.desktop.v1.SearchMemory.verify|verify} messages.
                 * @param message SearchMemory message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SearchMemory.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SearchMemory message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SearchMemory & evohime.desktop.v1.SearchMemory.$Shape} SearchMemory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SearchMemory & evohime.desktop.v1.SearchMemory.$Shape;

                /**
                 * Gets the type url for SearchMemory
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SearchMemory {

                /** Properties of a SearchMemory. */
                interface $Properties {

                    /** SearchMemory scopeKind */
                    scopeKind?: (string|null);

                    /** SearchMemory projectId */
                    projectId?: (string|null);

                    /** SearchMemory secondaryId */
                    secondaryId?: (string|null);

                    /** SearchMemory query */
                    query?: (string|null);

                    /** SearchMemory limit */
                    limit?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SearchMemory. */
                type $Shape = evohime.desktop.v1.SearchMemory.$Properties;
            }

            /**
             * Properties of an ArchiveMemory.
             * @deprecated Use evohime.desktop.v1.ArchiveMemory.$Properties instead.
             */
            interface IArchiveMemory extends evohime.desktop.v1.ArchiveMemory.$Properties {
            }

            /** Represents an ArchiveMemory. */
            class ArchiveMemory {

                /**
                 * Constructs a new ArchiveMemory.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ArchiveMemory.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ArchiveMemory id. */
                id: string;

                /** ArchiveMemory approvalId. */
                approvalId: string;

                /**
                 * Encodes the specified ArchiveMemory message. Does not implicitly {@link evohime.desktop.v1.ArchiveMemory.verify|verify} messages.
                 * @param message ArchiveMemory message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ArchiveMemory.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an ArchiveMemory message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ArchiveMemory & evohime.desktop.v1.ArchiveMemory.$Shape} ArchiveMemory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ArchiveMemory & evohime.desktop.v1.ArchiveMemory.$Shape;

                /**
                 * Gets the type url for ArchiveMemory
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ArchiveMemory {

                /** Properties of an ArchiveMemory. */
                interface $Properties {

                    /** ArchiveMemory id */
                    id?: (string|null);

                    /** ArchiveMemory approvalId */
                    approvalId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an ArchiveMemory. */
                type $Shape = evohime.desktop.v1.ArchiveMemory.$Properties;
            }

            /**
             * Properties of a ForgetMemory.
             * @deprecated Use evohime.desktop.v1.ForgetMemory.$Properties instead.
             */
            interface IForgetMemory extends evohime.desktop.v1.ForgetMemory.$Properties {
            }

            /** Represents a ForgetMemory. */
            class ForgetMemory {

                /**
                 * Constructs a new ForgetMemory.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ForgetMemory.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ForgetMemory id. */
                id: string;

                /** ForgetMemory approvalId. */
                approvalId: string;

                /**
                 * Encodes the specified ForgetMemory message. Does not implicitly {@link evohime.desktop.v1.ForgetMemory.verify|verify} messages.
                 * @param message ForgetMemory message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ForgetMemory.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ForgetMemory message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ForgetMemory & evohime.desktop.v1.ForgetMemory.$Shape} ForgetMemory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ForgetMemory & evohime.desktop.v1.ForgetMemory.$Shape;

                /**
                 * Gets the type url for ForgetMemory
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ForgetMemory {

                /** Properties of a ForgetMemory. */
                interface $Properties {

                    /** ForgetMemory id */
                    id?: (string|null);

                    /** ForgetMemory approvalId */
                    approvalId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ForgetMemory. */
                type $Shape = evohime.desktop.v1.ForgetMemory.$Properties;
            }

            /**
             * Properties of a GetMemory.
             * @deprecated Use evohime.desktop.v1.GetMemory.$Properties instead.
             */
            interface IGetMemory extends evohime.desktop.v1.GetMemory.$Properties {
            }

            /** Represents a GetMemory. */
            class GetMemory {

                /**
                 * Constructs a new GetMemory.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetMemory.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GetMemory id. */
                id: string;

                /**
                 * Encodes the specified GetMemory message. Does not implicitly {@link evohime.desktop.v1.GetMemory.verify|verify} messages.
                 * @param message GetMemory message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetMemory.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetMemory message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetMemory & evohime.desktop.v1.GetMemory.$Shape} GetMemory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetMemory & evohime.desktop.v1.GetMemory.$Shape;

                /**
                 * Gets the type url for GetMemory
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetMemory {

                /** Properties of a GetMemory. */
                interface $Properties {

                    /** GetMemory id */
                    id?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetMemory. */
                type $Shape = evohime.desktop.v1.GetMemory.$Properties;
            }

            /**
             * Properties of a ListMemoryPending.
             * @deprecated Use evohime.desktop.v1.ListMemoryPending.$Properties instead.
             */
            interface IListMemoryPending extends evohime.desktop.v1.ListMemoryPending.$Properties {
            }

            /** Represents a ListMemoryPending. */
            class ListMemoryPending {

                /**
                 * Constructs a new ListMemoryPending.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ListMemoryPending.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ListMemoryPending scopeKind. */
                scopeKind: string;

                /** ListMemoryPending projectId. */
                projectId: string;

                /** ListMemoryPending secondaryId. */
                secondaryId: string;

                /** ListMemoryPending limit. */
                limit: number;

                /** ListMemoryPending workspacePath. */
                workspacePath: string;

                /**
                 * Encodes the specified ListMemoryPending message. Does not implicitly {@link evohime.desktop.v1.ListMemoryPending.verify|verify} messages.
                 * @param message ListMemoryPending message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ListMemoryPending.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ListMemoryPending message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListMemoryPending & evohime.desktop.v1.ListMemoryPending.$Shape} ListMemoryPending
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ListMemoryPending & evohime.desktop.v1.ListMemoryPending.$Shape;

                /**
                 * Gets the type url for ListMemoryPending
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ListMemoryPending {

                /** Properties of a ListMemoryPending. */
                interface $Properties {

                    /** ListMemoryPending scopeKind */
                    scopeKind?: (string|null);

                    /** ListMemoryPending projectId */
                    projectId?: (string|null);

                    /** ListMemoryPending secondaryId */
                    secondaryId?: (string|null);

                    /** ListMemoryPending limit */
                    limit?: (number|null);

                    /** ListMemoryPending workspacePath */
                    workspacePath?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ListMemoryPending. */
                type $Shape = evohime.desktop.v1.ListMemoryPending.$Properties;
            }

            /**
             * Properties of a GetMemoryConflicts.
             * @deprecated Use evohime.desktop.v1.GetMemoryConflicts.$Properties instead.
             */
            interface IGetMemoryConflicts extends evohime.desktop.v1.GetMemoryConflicts.$Properties {
            }

            /** Represents a GetMemoryConflicts. */
            class GetMemoryConflicts {

                /**
                 * Constructs a new GetMemoryConflicts.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetMemoryConflicts.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GetMemoryConflicts scopeKind. */
                scopeKind: string;

                /** GetMemoryConflicts projectId. */
                projectId: string;

                /** GetMemoryConflicts secondaryId. */
                secondaryId: string;

                /** GetMemoryConflicts limit. */
                limit: number;

                /** GetMemoryConflicts workspacePath. */
                workspacePath: string;

                /**
                 * Encodes the specified GetMemoryConflicts message. Does not implicitly {@link evohime.desktop.v1.GetMemoryConflicts.verify|verify} messages.
                 * @param message GetMemoryConflicts message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetMemoryConflicts.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetMemoryConflicts message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetMemoryConflicts & evohime.desktop.v1.GetMemoryConflicts.$Shape} GetMemoryConflicts
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetMemoryConflicts & evohime.desktop.v1.GetMemoryConflicts.$Shape;

                /**
                 * Gets the type url for GetMemoryConflicts
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetMemoryConflicts {

                /** Properties of a GetMemoryConflicts. */
                interface $Properties {

                    /** GetMemoryConflicts scopeKind */
                    scopeKind?: (string|null);

                    /** GetMemoryConflicts projectId */
                    projectId?: (string|null);

                    /** GetMemoryConflicts secondaryId */
                    secondaryId?: (string|null);

                    /** GetMemoryConflicts limit */
                    limit?: (number|null);

                    /** GetMemoryConflicts workspacePath */
                    workspacePath?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetMemoryConflicts. */
                type $Shape = evohime.desktop.v1.GetMemoryConflicts.$Properties;
            }

            /**
             * Properties of a ConfirmMemory.
             * @deprecated Use evohime.desktop.v1.ConfirmMemory.$Properties instead.
             */
            interface IConfirmMemory extends evohime.desktop.v1.ConfirmMemory.$Properties {
            }

            /** Represents a ConfirmMemory. */
            class ConfirmMemory {

                /**
                 * Constructs a new ConfirmMemory.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ConfirmMemory.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ConfirmMemory ids. */
                ids: string[];

                /** ConfirmMemory approvalId. */
                approvalId: string;

                /** ConfirmMemory idempotencyKey. */
                idempotencyKey: string;

                /**
                 * Encodes the specified ConfirmMemory message. Does not implicitly {@link evohime.desktop.v1.ConfirmMemory.verify|verify} messages.
                 * @param message ConfirmMemory message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ConfirmMemory.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ConfirmMemory message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ConfirmMemory & evohime.desktop.v1.ConfirmMemory.$Shape} ConfirmMemory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ConfirmMemory & evohime.desktop.v1.ConfirmMemory.$Shape;

                /**
                 * Gets the type url for ConfirmMemory
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ConfirmMemory {

                /** Properties of a ConfirmMemory. */
                interface $Properties {

                    /** ConfirmMemory ids */
                    ids?: (string[]|null);

                    /** ConfirmMemory approvalId */
                    approvalId?: (string|null);

                    /** ConfirmMemory idempotencyKey */
                    idempotencyKey?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ConfirmMemory. */
                type $Shape = evohime.desktop.v1.ConfirmMemory.$Properties;
            }

            /**
             * Properties of a RejectMemory.
             * @deprecated Use evohime.desktop.v1.RejectMemory.$Properties instead.
             */
            interface IRejectMemory extends evohime.desktop.v1.RejectMemory.$Properties {
            }

            /** Represents a RejectMemory. */
            class RejectMemory {

                /**
                 * Constructs a new RejectMemory.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.RejectMemory.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** RejectMemory ids. */
                ids: string[];

                /** RejectMemory approvalId. */
                approvalId: string;

                /** RejectMemory idempotencyKey. */
                idempotencyKey: string;

                /**
                 * Encodes the specified RejectMemory message. Does not implicitly {@link evohime.desktop.v1.RejectMemory.verify|verify} messages.
                 * @param message RejectMemory message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.RejectMemory.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a RejectMemory message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RejectMemory & evohime.desktop.v1.RejectMemory.$Shape} RejectMemory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.RejectMemory & evohime.desktop.v1.RejectMemory.$Shape;

                /**
                 * Gets the type url for RejectMemory
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace RejectMemory {

                /** Properties of a RejectMemory. */
                interface $Properties {

                    /** RejectMemory ids */
                    ids?: (string[]|null);

                    /** RejectMemory approvalId */
                    approvalId?: (string|null);

                    /** RejectMemory idempotencyKey */
                    idempotencyKey?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a RejectMemory. */
                type $Shape = evohime.desktop.v1.RejectMemory.$Properties;
            }

            /**
             * Properties of a ReviseMemoryCandidate.
             * @deprecated Use evohime.desktop.v1.ReviseMemoryCandidate.$Properties instead.
             */
            interface IReviseMemoryCandidate extends evohime.desktop.v1.ReviseMemoryCandidate.$Properties {
            }

            /** Represents a ReviseMemoryCandidate. */
            class ReviseMemoryCandidate {

                /**
                 * Constructs a new ReviseMemoryCandidate.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ReviseMemoryCandidate.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ReviseMemoryCandidate id. */
                id: string;

                /** ReviseMemoryCandidate statement. */
                statement: string;

                /** ReviseMemoryCandidate sessionOnly. */
                sessionOnly: boolean;

                /** ReviseMemoryCandidate sessionId. */
                sessionId: string;

                /** ReviseMemoryCandidate approvalId. */
                approvalId: string;

                /** ReviseMemoryCandidate idempotencyKey. */
                idempotencyKey: string;

                /**
                 * Encodes the specified ReviseMemoryCandidate message. Does not implicitly {@link evohime.desktop.v1.ReviseMemoryCandidate.verify|verify} messages.
                 * @param message ReviseMemoryCandidate message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ReviseMemoryCandidate.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ReviseMemoryCandidate message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ReviseMemoryCandidate & evohime.desktop.v1.ReviseMemoryCandidate.$Shape} ReviseMemoryCandidate
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ReviseMemoryCandidate & evohime.desktop.v1.ReviseMemoryCandidate.$Shape;

                /**
                 * Gets the type url for ReviseMemoryCandidate
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ReviseMemoryCandidate {

                /** Properties of a ReviseMemoryCandidate. */
                interface $Properties {

                    /** ReviseMemoryCandidate id */
                    id?: (string|null);

                    /** ReviseMemoryCandidate statement */
                    statement?: (string|null);

                    /** ReviseMemoryCandidate sessionOnly */
                    sessionOnly?: (boolean|null);

                    /** ReviseMemoryCandidate sessionId */
                    sessionId?: (string|null);

                    /** ReviseMemoryCandidate approvalId */
                    approvalId?: (string|null);

                    /** ReviseMemoryCandidate idempotencyKey */
                    idempotencyKey?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ReviseMemoryCandidate. */
                type $Shape = evohime.desktop.v1.ReviseMemoryCandidate.$Properties;
            }

            /**
             * Properties of a SupersedeMemory.
             * @deprecated Use evohime.desktop.v1.SupersedeMemory.$Properties instead.
             */
            interface ISupersedeMemory extends evohime.desktop.v1.SupersedeMemory.$Properties {
            }

            /** Represents a SupersedeMemory. */
            class SupersedeMemory {

                /**
                 * Constructs a new SupersedeMemory.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SupersedeMemory.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SupersedeMemory oldId. */
                oldId: string;

                /** SupersedeMemory newId. */
                newId: string;

                /** SupersedeMemory reason. */
                reason: string;

                /** SupersedeMemory approvalId. */
                approvalId: string;

                /** SupersedeMemory idempotencyKey. */
                idempotencyKey: string;

                /**
                 * Encodes the specified SupersedeMemory message. Does not implicitly {@link evohime.desktop.v1.SupersedeMemory.verify|verify} messages.
                 * @param message SupersedeMemory message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SupersedeMemory.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SupersedeMemory message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SupersedeMemory & evohime.desktop.v1.SupersedeMemory.$Shape} SupersedeMemory
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SupersedeMemory & evohime.desktop.v1.SupersedeMemory.$Shape;

                /**
                 * Gets the type url for SupersedeMemory
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SupersedeMemory {

                /** Properties of a SupersedeMemory. */
                interface $Properties {

                    /** SupersedeMemory oldId */
                    oldId?: (string|null);

                    /** SupersedeMemory newId */
                    newId?: (string|null);

                    /** SupersedeMemory reason */
                    reason?: (string|null);

                    /** SupersedeMemory approvalId */
                    approvalId?: (string|null);

                    /** SupersedeMemory idempotencyKey */
                    idempotencyKey?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SupersedeMemory. */
                type $Shape = evohime.desktop.v1.SupersedeMemory.$Properties;
            }

            /**
             * Properties of an InstallCapability.
             * @deprecated Use evohime.desktop.v1.InstallCapability.$Properties instead.
             */
            interface IInstallCapability extends evohime.desktop.v1.InstallCapability.$Properties {
            }

            /** Represents an InstallCapability. */
            class InstallCapability {

                /**
                 * Constructs a new InstallCapability.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.InstallCapability.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** InstallCapability manifestJson. */
                manifestJson: string;

                /** InstallCapability installSource. */
                installSource: string;

                /** InstallCapability sourcePath. */
                sourcePath: string;

                /** InstallCapability expectedContentHash. */
                expectedContentHash: string;

                /**
                 * Encodes the specified InstallCapability message. Does not implicitly {@link evohime.desktop.v1.InstallCapability.verify|verify} messages.
                 * @param message InstallCapability message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.InstallCapability.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InstallCapability message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.InstallCapability & evohime.desktop.v1.InstallCapability.$Shape} InstallCapability
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.InstallCapability & evohime.desktop.v1.InstallCapability.$Shape;

                /**
                 * Gets the type url for InstallCapability
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace InstallCapability {

                /** Properties of an InstallCapability. */
                interface $Properties {

                    /** InstallCapability manifestJson */
                    manifestJson?: (string|null);

                    /** InstallCapability installSource */
                    installSource?: (string|null);

                    /** InstallCapability sourcePath */
                    sourcePath?: (string|null);

                    /** InstallCapability expectedContentHash */
                    expectedContentHash?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an InstallCapability. */
                type $Shape = evohime.desktop.v1.InstallCapability.$Properties;
            }

            /**
             * Properties of a ListCapabilities.
             * @deprecated Use evohime.desktop.v1.ListCapabilities.$Properties instead.
             */
            interface IListCapabilities extends evohime.desktop.v1.ListCapabilities.$Properties {
            }

            /** Represents a ListCapabilities. */
            class ListCapabilities {

                /**
                 * Constructs a new ListCapabilities.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ListCapabilities.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ListCapabilities limit. */
                limit: number;

                /**
                 * Encodes the specified ListCapabilities message. Does not implicitly {@link evohime.desktop.v1.ListCapabilities.verify|verify} messages.
                 * @param message ListCapabilities message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ListCapabilities.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ListCapabilities message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListCapabilities & evohime.desktop.v1.ListCapabilities.$Shape} ListCapabilities
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ListCapabilities & evohime.desktop.v1.ListCapabilities.$Shape;

                /**
                 * Gets the type url for ListCapabilities
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ListCapabilities {

                /** Properties of a ListCapabilities. */
                interface $Properties {

                    /** ListCapabilities limit */
                    limit?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ListCapabilities. */
                type $Shape = evohime.desktop.v1.ListCapabilities.$Properties;
            }

            /**
             * Properties of a MatchCapabilities.
             * @deprecated Use evohime.desktop.v1.MatchCapabilities.$Properties instead.
             */
            interface IMatchCapabilities extends evohime.desktop.v1.MatchCapabilities.$Properties {
            }

            /** Represents a MatchCapabilities. */
            class MatchCapabilities {

                /**
                 * Constructs a new MatchCapabilities.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.MatchCapabilities.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** MatchCapabilities intent. */
                intent: string;

                /** MatchCapabilities requiredTools. */
                requiredTools: string[];

                /** MatchCapabilities requiredDomains. */
                requiredDomains: string[];

                /** MatchCapabilities requestedRisk. */
                requestedRisk: string;

                /**
                 * Encodes the specified MatchCapabilities message. Does not implicitly {@link evohime.desktop.v1.MatchCapabilities.verify|verify} messages.
                 * @param message MatchCapabilities message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.MatchCapabilities.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a MatchCapabilities message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.MatchCapabilities & evohime.desktop.v1.MatchCapabilities.$Shape} MatchCapabilities
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.MatchCapabilities & evohime.desktop.v1.MatchCapabilities.$Shape;

                /**
                 * Gets the type url for MatchCapabilities
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace MatchCapabilities {

                /** Properties of a MatchCapabilities. */
                interface $Properties {

                    /** MatchCapabilities intent */
                    intent?: (string|null);

                    /** MatchCapabilities requiredTools */
                    requiredTools?: (string[]|null);

                    /** MatchCapabilities requiredDomains */
                    requiredDomains?: (string[]|null);

                    /** MatchCapabilities requestedRisk */
                    requestedRisk?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a MatchCapabilities. */
                type $Shape = evohime.desktop.v1.MatchCapabilities.$Properties;
            }

            /**
             * Properties of a RemoveCapability.
             * @deprecated Use evohime.desktop.v1.RemoveCapability.$Properties instead.
             */
            interface IRemoveCapability extends evohime.desktop.v1.RemoveCapability.$Properties {
            }

            /** Represents a RemoveCapability. */
            class RemoveCapability {

                /**
                 * Constructs a new RemoveCapability.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.RemoveCapability.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** RemoveCapability id. */
                id: string;

                /**
                 * Encodes the specified RemoveCapability message. Does not implicitly {@link evohime.desktop.v1.RemoveCapability.verify|verify} messages.
                 * @param message RemoveCapability message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.RemoveCapability.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a RemoveCapability message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RemoveCapability & evohime.desktop.v1.RemoveCapability.$Shape} RemoveCapability
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.RemoveCapability & evohime.desktop.v1.RemoveCapability.$Shape;

                /**
                 * Gets the type url for RemoveCapability
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace RemoveCapability {

                /** Properties of a RemoveCapability. */
                interface $Properties {

                    /** RemoveCapability id */
                    id?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a RemoveCapability. */
                type $Shape = evohime.desktop.v1.RemoveCapability.$Properties;
            }

            /**
             * Properties of a RequestChildHandoff.
             * @deprecated Use evohime.desktop.v1.RequestChildHandoff.$Properties instead.
             */
            interface IRequestChildHandoff extends evohime.desktop.v1.RequestChildHandoff.$Properties {
            }

            /** Represents a RequestChildHandoff. */
            class RequestChildHandoff {

                /**
                 * Constructs a new RequestChildHandoff.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.RequestChildHandoff.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** RequestChildHandoff handoffId. */
                handoffId: string;

                /** RequestChildHandoff taskId. */
                taskId: string;

                /** RequestChildHandoff kind. */
                kind: string;

                /** RequestChildHandoff fromRole. */
                fromRole: string;

                /** RequestChildHandoff fromName. */
                fromName: string;

                /** RequestChildHandoff toRole. */
                toRole: string;

                /** RequestChildHandoff toName. */
                toName: string;

                /** RequestChildHandoff purpose. */
                purpose: string;

                /** RequestChildHandoff payload. */
                payload: { [k: string]: string };

                /** RequestChildHandoff sequence. */
                sequence: number;

                /**
                 * Encodes the specified RequestChildHandoff message. Does not implicitly {@link evohime.desktop.v1.RequestChildHandoff.verify|verify} messages.
                 * @param message RequestChildHandoff message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.RequestChildHandoff.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a RequestChildHandoff message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RequestChildHandoff & evohime.desktop.v1.RequestChildHandoff.$Shape} RequestChildHandoff
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.RequestChildHandoff & evohime.desktop.v1.RequestChildHandoff.$Shape;

                /**
                 * Gets the type url for RequestChildHandoff
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace RequestChildHandoff {

                /** Properties of a RequestChildHandoff. */
                interface $Properties {

                    /** RequestChildHandoff handoffId */
                    handoffId?: (string|null);

                    /** RequestChildHandoff taskId */
                    taskId?: (string|null);

                    /** RequestChildHandoff kind */
                    kind?: (string|null);

                    /** RequestChildHandoff fromRole */
                    fromRole?: (string|null);

                    /** RequestChildHandoff fromName */
                    fromName?: (string|null);

                    /** RequestChildHandoff toRole */
                    toRole?: (string|null);

                    /** RequestChildHandoff toName */
                    toName?: (string|null);

                    /** RequestChildHandoff purpose */
                    purpose?: (string|null);

                    /** RequestChildHandoff payload */
                    payload?: ({ [k: string]: string }|null);

                    /** RequestChildHandoff sequence */
                    sequence?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a RequestChildHandoff. */
                type $Shape = evohime.desktop.v1.RequestChildHandoff.$Properties;
            }

            /**
             * Properties of a ListChildHandoffs.
             * @deprecated Use evohime.desktop.v1.ListChildHandoffs.$Properties instead.
             */
            interface IListChildHandoffs extends evohime.desktop.v1.ListChildHandoffs.$Properties {
            }

            /** Represents a ListChildHandoffs. */
            class ListChildHandoffs {

                /**
                 * Constructs a new ListChildHandoffs.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ListChildHandoffs.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ListChildHandoffs taskId. */
                taskId: string;

                /** ListChildHandoffs limit. */
                limit: number;

                /**
                 * Encodes the specified ListChildHandoffs message. Does not implicitly {@link evohime.desktop.v1.ListChildHandoffs.verify|verify} messages.
                 * @param message ListChildHandoffs message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ListChildHandoffs.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ListChildHandoffs message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListChildHandoffs & evohime.desktop.v1.ListChildHandoffs.$Shape} ListChildHandoffs
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ListChildHandoffs & evohime.desktop.v1.ListChildHandoffs.$Shape;

                /**
                 * Gets the type url for ListChildHandoffs
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ListChildHandoffs {

                /** Properties of a ListChildHandoffs. */
                interface $Properties {

                    /** ListChildHandoffs taskId */
                    taskId?: (string|null);

                    /** ListChildHandoffs limit */
                    limit?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ListChildHandoffs. */
                type $Shape = evohime.desktop.v1.ListChildHandoffs.$Properties;
            }

            /**
             * Properties of a SubmitChildRequest.
             * @deprecated Use evohime.desktop.v1.SubmitChildRequest.$Properties instead.
             */
            interface ISubmitChildRequest extends evohime.desktop.v1.SubmitChildRequest.$Properties {
            }

            /** Represents a SubmitChildRequest. */
            class SubmitChildRequest {

                /**
                 * Constructs a new SubmitChildRequest.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SubmitChildRequest.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SubmitChildRequest childTaskId. */
                childTaskId: string;

                /** SubmitChildRequest parentTaskId. */
                parentTaskId: string;

                /** SubmitChildRequest role. */
                role: string;

                /** SubmitChildRequest kind. */
                kind: string;

                /** SubmitChildRequest reducedContext. */
                reducedContext: string[];

                /** SubmitChildRequest maxOutputBytes. */
                maxOutputBytes: number;

                /** SubmitChildRequest requestedCapabilities. */
                requestedCapabilities: string[];

                /** SubmitChildRequest parentIsChild. */
                parentIsChild: boolean;

                /**
                 * Encodes the specified SubmitChildRequest message. Does not implicitly {@link evohime.desktop.v1.SubmitChildRequest.verify|verify} messages.
                 * @param message SubmitChildRequest message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SubmitChildRequest.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SubmitChildRequest message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SubmitChildRequest & evohime.desktop.v1.SubmitChildRequest.$Shape} SubmitChildRequest
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SubmitChildRequest & evohime.desktop.v1.SubmitChildRequest.$Shape;

                /**
                 * Gets the type url for SubmitChildRequest
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SubmitChildRequest {

                /** Properties of a SubmitChildRequest. */
                interface $Properties {

                    /** SubmitChildRequest childTaskId */
                    childTaskId?: (string|null);

                    /** SubmitChildRequest parentTaskId */
                    parentTaskId?: (string|null);

                    /** SubmitChildRequest role */
                    role?: (string|null);

                    /** SubmitChildRequest kind */
                    kind?: (string|null);

                    /** SubmitChildRequest reducedContext */
                    reducedContext?: (string[]|null);

                    /** SubmitChildRequest maxOutputBytes */
                    maxOutputBytes?: (number|null);

                    /** SubmitChildRequest requestedCapabilities */
                    requestedCapabilities?: (string[]|null);

                    /** SubmitChildRequest parentIsChild */
                    parentIsChild?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SubmitChildRequest. */
                type $Shape = evohime.desktop.v1.SubmitChildRequest.$Properties;
            }

            /**
             * Properties of a SubmitChildReport.
             * @deprecated Use evohime.desktop.v1.SubmitChildReport.$Properties instead.
             */
            interface ISubmitChildReport extends evohime.desktop.v1.SubmitChildReport.$Properties {
            }

            /** Represents a SubmitChildReport. */
            class SubmitChildReport {

                /**
                 * Constructs a new SubmitChildReport.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SubmitChildReport.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SubmitChildReport childTaskId. */
                childTaskId: string;

                /** SubmitChildReport status. */
                status: string;

                /** SubmitChildReport summary. */
                summary: string;

                /** SubmitChildReport findings. */
                findings: string[];

                /** SubmitChildReport sources. */
                sources: string[];

                /** SubmitChildReport confidencePercent. */
                confidencePercent: number;

                /**
                 * Encodes the specified SubmitChildReport message. Does not implicitly {@link evohime.desktop.v1.SubmitChildReport.verify|verify} messages.
                 * @param message SubmitChildReport message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SubmitChildReport.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SubmitChildReport message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SubmitChildReport & evohime.desktop.v1.SubmitChildReport.$Shape} SubmitChildReport
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SubmitChildReport & evohime.desktop.v1.SubmitChildReport.$Shape;

                /**
                 * Gets the type url for SubmitChildReport
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SubmitChildReport {

                /** Properties of a SubmitChildReport. */
                interface $Properties {

                    /** SubmitChildReport childTaskId */
                    childTaskId?: (string|null);

                    /** SubmitChildReport status */
                    status?: (string|null);

                    /** SubmitChildReport summary */
                    summary?: (string|null);

                    /** SubmitChildReport findings */
                    findings?: (string[]|null);

                    /** SubmitChildReport sources */
                    sources?: (string[]|null);

                    /** SubmitChildReport confidencePercent */
                    confidencePercent?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SubmitChildReport. */
                type $Shape = evohime.desktop.v1.SubmitChildReport.$Properties;
            }

            /**
             * Properties of a GetCapabilitySelection.
             * @deprecated Use evohime.desktop.v1.GetCapabilitySelection.$Properties instead.
             */
            interface IGetCapabilitySelection extends evohime.desktop.v1.GetCapabilitySelection.$Properties {
            }

            /** Represents a GetCapabilitySelection. */
            class GetCapabilitySelection {

                /**
                 * Constructs a new GetCapabilitySelection.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetCapabilitySelection.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GetCapabilitySelection taskId. */
                taskId: string;

                /** GetCapabilitySelection intent. */
                intent: string;

                /** GetCapabilitySelection requiredTools. */
                requiredTools: string[];

                /** GetCapabilitySelection requiredDomains. */
                requiredDomains: string[];

                /** GetCapabilitySelection requestedRisk. */
                requestedRisk: string;

                /**
                 * Encodes the specified GetCapabilitySelection message. Does not implicitly {@link evohime.desktop.v1.GetCapabilitySelection.verify|verify} messages.
                 * @param message GetCapabilitySelection message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetCapabilitySelection.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetCapabilitySelection message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetCapabilitySelection & evohime.desktop.v1.GetCapabilitySelection.$Shape} GetCapabilitySelection
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetCapabilitySelection & evohime.desktop.v1.GetCapabilitySelection.$Shape;

                /**
                 * Gets the type url for GetCapabilitySelection
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetCapabilitySelection {

                /** Properties of a GetCapabilitySelection. */
                interface $Properties {

                    /** GetCapabilitySelection taskId */
                    taskId?: (string|null);

                    /** GetCapabilitySelection intent */
                    intent?: (string|null);

                    /** GetCapabilitySelection requiredTools */
                    requiredTools?: (string[]|null);

                    /** GetCapabilitySelection requiredDomains */
                    requiredDomains?: (string[]|null);

                    /** GetCapabilitySelection requestedRisk */
                    requestedRisk?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetCapabilitySelection. */
                type $Shape = evohime.desktop.v1.GetCapabilitySelection.$Properties;
            }

            /**
             * Properties of a PinCapabilitySelection.
             * @deprecated Use evohime.desktop.v1.PinCapabilitySelection.$Properties instead.
             */
            interface IPinCapabilitySelection extends evohime.desktop.v1.PinCapabilitySelection.$Properties {
            }

            /** Represents a PinCapabilitySelection. */
            class PinCapabilitySelection {

                /**
                 * Constructs a new PinCapabilitySelection.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.PinCapabilitySelection.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** PinCapabilitySelection taskId. */
                taskId: string;

                /**
                 * Encodes the specified PinCapabilitySelection message. Does not implicitly {@link evohime.desktop.v1.PinCapabilitySelection.verify|verify} messages.
                 * @param message PinCapabilitySelection message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.PinCapabilitySelection.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a PinCapabilitySelection message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.PinCapabilitySelection & evohime.desktop.v1.PinCapabilitySelection.$Shape} PinCapabilitySelection
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.PinCapabilitySelection & evohime.desktop.v1.PinCapabilitySelection.$Shape;

                /**
                 * Gets the type url for PinCapabilitySelection
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace PinCapabilitySelection {

                /** Properties of a PinCapabilitySelection. */
                interface $Properties {

                    /** PinCapabilitySelection taskId */
                    taskId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a PinCapabilitySelection. */
                type $Shape = evohime.desktop.v1.PinCapabilitySelection.$Properties;
            }

            /**
             * Properties of a ReplaceCapabilitySelection.
             * @deprecated Use evohime.desktop.v1.ReplaceCapabilitySelection.$Properties instead.
             */
            interface IReplaceCapabilitySelection extends evohime.desktop.v1.ReplaceCapabilitySelection.$Properties {
            }

            /** Represents a ReplaceCapabilitySelection. */
            class ReplaceCapabilitySelection {

                /**
                 * Constructs a new ReplaceCapabilitySelection.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ReplaceCapabilitySelection.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ReplaceCapabilitySelection taskId. */
                taskId: string;

                /** ReplaceCapabilitySelection manifestName. */
                manifestName: string;

                /** ReplaceCapabilitySelection intent. */
                intent: string;

                /** ReplaceCapabilitySelection requiredTools. */
                requiredTools: string[];

                /** ReplaceCapabilitySelection requiredDomains. */
                requiredDomains: string[];

                /** ReplaceCapabilitySelection requestedRisk. */
                requestedRisk: string;

                /**
                 * Encodes the specified ReplaceCapabilitySelection message. Does not implicitly {@link evohime.desktop.v1.ReplaceCapabilitySelection.verify|verify} messages.
                 * @param message ReplaceCapabilitySelection message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ReplaceCapabilitySelection.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ReplaceCapabilitySelection message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ReplaceCapabilitySelection & evohime.desktop.v1.ReplaceCapabilitySelection.$Shape} ReplaceCapabilitySelection
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ReplaceCapabilitySelection & evohime.desktop.v1.ReplaceCapabilitySelection.$Shape;

                /**
                 * Gets the type url for ReplaceCapabilitySelection
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ReplaceCapabilitySelection {

                /** Properties of a ReplaceCapabilitySelection. */
                interface $Properties {

                    /** ReplaceCapabilitySelection taskId */
                    taskId?: (string|null);

                    /** ReplaceCapabilitySelection manifestName */
                    manifestName?: (string|null);

                    /** ReplaceCapabilitySelection intent */
                    intent?: (string|null);

                    /** ReplaceCapabilitySelection requiredTools */
                    requiredTools?: (string[]|null);

                    /** ReplaceCapabilitySelection requiredDomains */
                    requiredDomains?: (string[]|null);

                    /** ReplaceCapabilitySelection requestedRisk */
                    requestedRisk?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ReplaceCapabilitySelection. */
                type $Shape = evohime.desktop.v1.ReplaceCapabilitySelection.$Properties;
            }

            /**
             * Properties of a SubmitFeedback.
             * @deprecated Use evohime.desktop.v1.SubmitFeedback.$Properties instead.
             */
            interface ISubmitFeedback extends evohime.desktop.v1.SubmitFeedback.$Properties {
            }

            /** Represents a SubmitFeedback. */
            class SubmitFeedback {

                /**
                 * Constructs a new SubmitFeedback.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SubmitFeedback.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SubmitFeedback runId. */
                runId: string;

                /** SubmitFeedback taskId. */
                taskId: string;

                /** SubmitFeedback subjectRef. */
                subjectRef: string;

                /** SubmitFeedback signal. */
                signal: string;

                /** SubmitFeedback correction. */
                correction: string;

                /** SubmitFeedback rejectionReason. */
                rejectionReason: string;

                /** SubmitFeedback outcome. */
                outcome: string;

                /**
                 * Encodes the specified SubmitFeedback message. Does not implicitly {@link evohime.desktop.v1.SubmitFeedback.verify|verify} messages.
                 * @param message SubmitFeedback message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SubmitFeedback.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SubmitFeedback message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SubmitFeedback & evohime.desktop.v1.SubmitFeedback.$Shape} SubmitFeedback
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SubmitFeedback & evohime.desktop.v1.SubmitFeedback.$Shape;

                /**
                 * Gets the type url for SubmitFeedback
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SubmitFeedback {

                /** Properties of a SubmitFeedback. */
                interface $Properties {

                    /** SubmitFeedback runId */
                    runId?: (string|null);

                    /** SubmitFeedback taskId */
                    taskId?: (string|null);

                    /** SubmitFeedback subjectRef */
                    subjectRef?: (string|null);

                    /** SubmitFeedback signal */
                    signal?: (string|null);

                    /** SubmitFeedback correction */
                    correction?: (string|null);

                    /** SubmitFeedback rejectionReason */
                    rejectionReason?: (string|null);

                    /** SubmitFeedback outcome */
                    outcome?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SubmitFeedback. */
                type $Shape = evohime.desktop.v1.SubmitFeedback.$Properties;
            }

            /**
             * Properties of a ListFeedback.
             * @deprecated Use evohime.desktop.v1.ListFeedback.$Properties instead.
             */
            interface IListFeedback extends evohime.desktop.v1.ListFeedback.$Properties {
            }

            /** Represents a ListFeedback. */
            class ListFeedback {

                /**
                 * Constructs a new ListFeedback.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ListFeedback.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ListFeedback runId. */
                runId: string;

                /** ListFeedback limit. */
                limit: number;

                /**
                 * Encodes the specified ListFeedback message. Does not implicitly {@link evohime.desktop.v1.ListFeedback.verify|verify} messages.
                 * @param message ListFeedback message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ListFeedback.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ListFeedback message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListFeedback & evohime.desktop.v1.ListFeedback.$Shape} ListFeedback
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ListFeedback & evohime.desktop.v1.ListFeedback.$Shape;

                /**
                 * Gets the type url for ListFeedback
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ListFeedback {

                /** Properties of a ListFeedback. */
                interface $Properties {

                    /** ListFeedback runId */
                    runId?: (string|null);

                    /** ListFeedback limit */
                    limit?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ListFeedback. */
                type $Shape = evohime.desktop.v1.ListFeedback.$Properties;
            }

            /**
             * Properties of an IndexWorkspace.
             * @deprecated Use evohime.desktop.v1.IndexWorkspace.$Properties instead.
             */
            interface IIndexWorkspace extends evohime.desktop.v1.IndexWorkspace.$Properties {
            }

            /** Represents an IndexWorkspace. */
            class IndexWorkspace {

                /**
                 * Constructs a new IndexWorkspace.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.IndexWorkspace.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** IndexWorkspace workspacePath. */
                workspacePath: string;

                /** IndexWorkspace enableEmbeddings. */
                enableEmbeddings: boolean;

                /**
                 * Encodes the specified IndexWorkspace message. Does not implicitly {@link evohime.desktop.v1.IndexWorkspace.verify|verify} messages.
                 * @param message IndexWorkspace message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.IndexWorkspace.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an IndexWorkspace message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.IndexWorkspace & evohime.desktop.v1.IndexWorkspace.$Shape} IndexWorkspace
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.IndexWorkspace & evohime.desktop.v1.IndexWorkspace.$Shape;

                /**
                 * Gets the type url for IndexWorkspace
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace IndexWorkspace {

                /** Properties of an IndexWorkspace. */
                interface $Properties {

                    /** IndexWorkspace workspacePath */
                    workspacePath?: (string|null);

                    /** IndexWorkspace enableEmbeddings */
                    enableEmbeddings?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an IndexWorkspace. */
                type $Shape = evohime.desktop.v1.IndexWorkspace.$Properties;
            }

            /**
             * Properties of a RebuildIndex.
             * @deprecated Use evohime.desktop.v1.RebuildIndex.$Properties instead.
             */
            interface IRebuildIndex extends evohime.desktop.v1.RebuildIndex.$Properties {
            }

            /** Represents a RebuildIndex. */
            class RebuildIndex {

                /**
                 * Constructs a new RebuildIndex.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.RebuildIndex.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** RebuildIndex workspacePath. */
                workspacePath: string;

                /** RebuildIndex enableEmbeddings. */
                enableEmbeddings: boolean;

                /**
                 * Encodes the specified RebuildIndex message. Does not implicitly {@link evohime.desktop.v1.RebuildIndex.verify|verify} messages.
                 * @param message RebuildIndex message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.RebuildIndex.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a RebuildIndex message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.RebuildIndex & evohime.desktop.v1.RebuildIndex.$Shape} RebuildIndex
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.RebuildIndex & evohime.desktop.v1.RebuildIndex.$Shape;

                /**
                 * Gets the type url for RebuildIndex
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace RebuildIndex {

                /** Properties of a RebuildIndex. */
                interface $Properties {

                    /** RebuildIndex workspacePath */
                    workspacePath?: (string|null);

                    /** RebuildIndex enableEmbeddings */
                    enableEmbeddings?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a RebuildIndex. */
                type $Shape = evohime.desktop.v1.RebuildIndex.$Properties;
            }

            /**
             * Properties of a SearchWorkspaceKnowledge.
             * @deprecated Use evohime.desktop.v1.SearchWorkspaceKnowledge.$Properties instead.
             */
            interface ISearchWorkspaceKnowledge extends evohime.desktop.v1.SearchWorkspaceKnowledge.$Properties {
            }

            /** Represents a SearchWorkspaceKnowledge. */
            class SearchWorkspaceKnowledge {

                /**
                 * Constructs a new SearchWorkspaceKnowledge.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SearchWorkspaceKnowledge.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SearchWorkspaceKnowledge workspacePath. */
                workspacePath: string;

                /** SearchWorkspaceKnowledge query. */
                query: string;

                /** SearchWorkspaceKnowledge pathFilter. */
                pathFilter: string;

                /** SearchWorkspaceKnowledge languageFilter. */
                languageFilter: string;

                /** SearchWorkspaceKnowledge hybrid. */
                hybrid: boolean;

                /**
                 * Encodes the specified SearchWorkspaceKnowledge message. Does not implicitly {@link evohime.desktop.v1.SearchWorkspaceKnowledge.verify|verify} messages.
                 * @param message SearchWorkspaceKnowledge message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SearchWorkspaceKnowledge.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SearchWorkspaceKnowledge message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SearchWorkspaceKnowledge & evohime.desktop.v1.SearchWorkspaceKnowledge.$Shape} SearchWorkspaceKnowledge
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SearchWorkspaceKnowledge & evohime.desktop.v1.SearchWorkspaceKnowledge.$Shape;

                /**
                 * Gets the type url for SearchWorkspaceKnowledge
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SearchWorkspaceKnowledge {

                /** Properties of a SearchWorkspaceKnowledge. */
                interface $Properties {

                    /** SearchWorkspaceKnowledge workspacePath */
                    workspacePath?: (string|null);

                    /** SearchWorkspaceKnowledge query */
                    query?: (string|null);

                    /** SearchWorkspaceKnowledge pathFilter */
                    pathFilter?: (string|null);

                    /** SearchWorkspaceKnowledge languageFilter */
                    languageFilter?: (string|null);

                    /** SearchWorkspaceKnowledge hybrid */
                    hybrid?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SearchWorkspaceKnowledge. */
                type $Shape = evohime.desktop.v1.SearchWorkspaceKnowledge.$Properties;
            }

            /**
             * Properties of a GetIndexStatus.
             * @deprecated Use evohime.desktop.v1.GetIndexStatus.$Properties instead.
             */
            interface IGetIndexStatus extends evohime.desktop.v1.GetIndexStatus.$Properties {
            }

            /** Represents a GetIndexStatus. */
            class GetIndexStatus {

                /**
                 * Constructs a new GetIndexStatus.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetIndexStatus.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GetIndexStatus workspacePath. */
                workspacePath: string;

                /**
                 * Encodes the specified GetIndexStatus message. Does not implicitly {@link evohime.desktop.v1.GetIndexStatus.verify|verify} messages.
                 * @param message GetIndexStatus message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetIndexStatus.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetIndexStatus message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetIndexStatus & evohime.desktop.v1.GetIndexStatus.$Shape} GetIndexStatus
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetIndexStatus & evohime.desktop.v1.GetIndexStatus.$Shape;

                /**
                 * Gets the type url for GetIndexStatus
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetIndexStatus {

                /** Properties of a GetIndexStatus. */
                interface $Properties {

                    /** GetIndexStatus workspacePath */
                    workspacePath?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetIndexStatus. */
                type $Shape = evohime.desktop.v1.GetIndexStatus.$Properties;
            }

            /**
             * Properties of a CancelWorkspaceIndex.
             * @deprecated Use evohime.desktop.v1.CancelWorkspaceIndex.$Properties instead.
             */
            interface ICancelWorkspaceIndex extends evohime.desktop.v1.CancelWorkspaceIndex.$Properties {
            }

            /** Represents a CancelWorkspaceIndex. */
            class CancelWorkspaceIndex {

                /**
                 * Constructs a new CancelWorkspaceIndex.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.CancelWorkspaceIndex.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** CancelWorkspaceIndex workspacePath. */
                workspacePath: string;

                /**
                 * Encodes the specified CancelWorkspaceIndex message. Does not implicitly {@link evohime.desktop.v1.CancelWorkspaceIndex.verify|verify} messages.
                 * @param message CancelWorkspaceIndex message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.CancelWorkspaceIndex.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CancelWorkspaceIndex message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.CancelWorkspaceIndex & evohime.desktop.v1.CancelWorkspaceIndex.$Shape} CancelWorkspaceIndex
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.CancelWorkspaceIndex & evohime.desktop.v1.CancelWorkspaceIndex.$Shape;

                /**
                 * Gets the type url for CancelWorkspaceIndex
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace CancelWorkspaceIndex {

                /** Properties of a CancelWorkspaceIndex. */
                interface $Properties {

                    /** CancelWorkspaceIndex workspacePath */
                    workspacePath?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a CancelWorkspaceIndex. */
                type $Shape = evohime.desktop.v1.CancelWorkspaceIndex.$Properties;
            }

            /**
             * Properties of a GetContextLedger.
             * @deprecated Use evohime.desktop.v1.GetContextLedger.$Properties instead.
             */
            interface IGetContextLedger extends evohime.desktop.v1.GetContextLedger.$Properties {
            }

            /** Represents a GetContextLedger. */
            class GetContextLedger {

                /**
                 * Constructs a new GetContextLedger.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetContextLedger.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GetContextLedger taskId. */
                taskId: string;

                /** GetContextLedger limit. */
                limit: number;

                /**
                 * Encodes the specified GetContextLedger message. Does not implicitly {@link evohime.desktop.v1.GetContextLedger.verify|verify} messages.
                 * @param message GetContextLedger message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetContextLedger.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetContextLedger message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetContextLedger & evohime.desktop.v1.GetContextLedger.$Shape} GetContextLedger
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetContextLedger & evohime.desktop.v1.GetContextLedger.$Shape;

                /**
                 * Gets the type url for GetContextLedger
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetContextLedger {

                /** Properties of a GetContextLedger. */
                interface $Properties {

                    /** GetContextLedger taskId */
                    taskId?: (string|null);

                    /** GetContextLedger limit */
                    limit?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetContextLedger. */
                type $Shape = evohime.desktop.v1.GetContextLedger.$Properties;
            }

            /**
             * Properties of a ListTaskScratchpad.
             * @deprecated Use evohime.desktop.v1.ListTaskScratchpad.$Properties instead.
             */
            interface IListTaskScratchpad extends evohime.desktop.v1.ListTaskScratchpad.$Properties {
            }

            /** Represents a ListTaskScratchpad. */
            class ListTaskScratchpad {

                /**
                 * Constructs a new ListTaskScratchpad.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ListTaskScratchpad.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ListTaskScratchpad taskId. */
                taskId: string;

                /** ListTaskScratchpad category. */
                category: string;

                /** ListTaskScratchpad status. */
                status: string;

                /** ListTaskScratchpad limit. */
                limit: number;

                /**
                 * Encodes the specified ListTaskScratchpad message. Does not implicitly {@link evohime.desktop.v1.ListTaskScratchpad.verify|verify} messages.
                 * @param message ListTaskScratchpad message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ListTaskScratchpad.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ListTaskScratchpad message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListTaskScratchpad & evohime.desktop.v1.ListTaskScratchpad.$Shape} ListTaskScratchpad
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ListTaskScratchpad & evohime.desktop.v1.ListTaskScratchpad.$Shape;

                /**
                 * Gets the type url for ListTaskScratchpad
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ListTaskScratchpad {

                /** Properties of a ListTaskScratchpad. */
                interface $Properties {

                    /** ListTaskScratchpad taskId */
                    taskId?: (string|null);

                    /** ListTaskScratchpad category */
                    category?: (string|null);

                    /** ListTaskScratchpad status */
                    status?: (string|null);

                    /** ListTaskScratchpad limit */
                    limit?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ListTaskScratchpad. */
                type $Shape = evohime.desktop.v1.ListTaskScratchpad.$Properties;
            }

            /**
             * Properties of a ClearTaskScratchpad.
             * @deprecated Use evohime.desktop.v1.ClearTaskScratchpad.$Properties instead.
             */
            interface IClearTaskScratchpad extends evohime.desktop.v1.ClearTaskScratchpad.$Properties {
            }

            /** Represents a ClearTaskScratchpad. */
            class ClearTaskScratchpad {

                /**
                 * Constructs a new ClearTaskScratchpad.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ClearTaskScratchpad.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ClearTaskScratchpad taskId. */
                taskId: string;

                /**
                 * Encodes the specified ClearTaskScratchpad message. Does not implicitly {@link evohime.desktop.v1.ClearTaskScratchpad.verify|verify} messages.
                 * @param message ClearTaskScratchpad message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ClearTaskScratchpad.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ClearTaskScratchpad message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ClearTaskScratchpad & evohime.desktop.v1.ClearTaskScratchpad.$Shape} ClearTaskScratchpad
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ClearTaskScratchpad & evohime.desktop.v1.ClearTaskScratchpad.$Shape;

                /**
                 * Gets the type url for ClearTaskScratchpad
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ClearTaskScratchpad {

                /** Properties of a ClearTaskScratchpad. */
                interface $Properties {

                    /** ClearTaskScratchpad taskId */
                    taskId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ClearTaskScratchpad. */
                type $Shape = evohime.desktop.v1.ClearTaskScratchpad.$Properties;
            }

            /**
             * Properties of a SummarizeContextNow.
             * @deprecated Use evohime.desktop.v1.SummarizeContextNow.$Properties instead.
             */
            interface ISummarizeContextNow extends evohime.desktop.v1.SummarizeContextNow.$Properties {
            }

            /** Represents a SummarizeContextNow. */
            class SummarizeContextNow {

                /**
                 * Constructs a new SummarizeContextNow.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SummarizeContextNow.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SummarizeContextNow taskId. */
                taskId: string;

                /**
                 * Encodes the specified SummarizeContextNow message. Does not implicitly {@link evohime.desktop.v1.SummarizeContextNow.verify|verify} messages.
                 * @param message SummarizeContextNow message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SummarizeContextNow.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SummarizeContextNow message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SummarizeContextNow & evohime.desktop.v1.SummarizeContextNow.$Shape} SummarizeContextNow
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SummarizeContextNow & evohime.desktop.v1.SummarizeContextNow.$Shape;

                /**
                 * Gets the type url for SummarizeContextNow
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SummarizeContextNow {

                /** Properties of a SummarizeContextNow. */
                interface $Properties {

                    /** SummarizeContextNow taskId */
                    taskId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SummarizeContextNow. */
                type $Shape = evohime.desktop.v1.SummarizeContextNow.$Properties;
            }

            /**
             * Properties of a PinContextItem.
             * @deprecated Use evohime.desktop.v1.PinContextItem.$Properties instead.
             */
            interface IPinContextItem extends evohime.desktop.v1.PinContextItem.$Properties {
            }

            /** Represents a PinContextItem. */
            class PinContextItem {

                /**
                 * Constructs a new PinContextItem.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.PinContextItem.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** PinContextItem taskId. */
                taskId: string;

                /** PinContextItem itemId. */
                itemId: string;

                /** PinContextItem pinned. */
                pinned: boolean;

                /**
                 * Encodes the specified PinContextItem message. Does not implicitly {@link evohime.desktop.v1.PinContextItem.verify|verify} messages.
                 * @param message PinContextItem message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.PinContextItem.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a PinContextItem message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.PinContextItem & evohime.desktop.v1.PinContextItem.$Shape} PinContextItem
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.PinContextItem & evohime.desktop.v1.PinContextItem.$Shape;

                /**
                 * Gets the type url for PinContextItem
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace PinContextItem {

                /** Properties of a PinContextItem. */
                interface $Properties {

                    /** PinContextItem taskId */
                    taskId?: (string|null);

                    /** PinContextItem itemId */
                    itemId?: (string|null);

                    /** PinContextItem pinned */
                    pinned?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a PinContextItem. */
                type $Shape = evohime.desktop.v1.PinContextItem.$Properties;
            }

            /**
             * Properties of a ReadContextArtifact.
             * @deprecated Use evohime.desktop.v1.ReadContextArtifact.$Properties instead.
             */
            interface IReadContextArtifact extends evohime.desktop.v1.ReadContextArtifact.$Properties {
            }

            /** Represents a ReadContextArtifact. */
            class ReadContextArtifact {

                /**
                 * Constructs a new ReadContextArtifact.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ReadContextArtifact.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ReadContextArtifact taskId. */
                taskId: string;

                /** ReadContextArtifact locator. */
                locator: string;

                /**
                 * Encodes the specified ReadContextArtifact message. Does not implicitly {@link evohime.desktop.v1.ReadContextArtifact.verify|verify} messages.
                 * @param message ReadContextArtifact message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ReadContextArtifact.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ReadContextArtifact message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ReadContextArtifact & evohime.desktop.v1.ReadContextArtifact.$Shape} ReadContextArtifact
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ReadContextArtifact & evohime.desktop.v1.ReadContextArtifact.$Shape;

                /**
                 * Gets the type url for ReadContextArtifact
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ReadContextArtifact {

                /** Properties of a ReadContextArtifact. */
                interface $Properties {

                    /** ReadContextArtifact taskId */
                    taskId?: (string|null);

                    /** ReadContextArtifact locator */
                    locator?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ReadContextArtifact. */
                type $Shape = evohime.desktop.v1.ReadContextArtifact.$Properties;
            }

            /** ListeningState enum. */
            enum ListeningState {

                /** LISTENING_STATE_UNKNOWN value */
                LISTENING_STATE_UNKNOWN = 0,

                /** LISTENING_STATE_STOPPED value */
                LISTENING_STATE_STOPPED = 1,

                /** LISTENING_STATE_STARTING value */
                LISTENING_STATE_STARTING = 2,

                /** LISTENING_STATE_LISTENING value */
                LISTENING_STATE_LISTENING = 3,

                /** LISTENING_STATE_PAUSED_BY_USER value */
                LISTENING_STATE_PAUSED_BY_USER = 4,

                /** LISTENING_STATE_PAUSED_BY_POLICY value */
                LISTENING_STATE_PAUSED_BY_POLICY = 5,

                /** LISTENING_STATE_DEVICE_CONFLICT value */
                LISTENING_STATE_DEVICE_CONFLICT = 6,

                /** LISTENING_STATE_DEVICE_DISCONNECTED value */
                LISTENING_STATE_DEVICE_DISCONNECTED = 7,

                /** LISTENING_STATE_ENGINE_UNAVAILABLE value */
                LISTENING_STATE_ENGINE_UNAVAILABLE = 8,

                /** LISTENING_STATE_DENIED value */
                LISTENING_STATE_DENIED = 9
            }

            /** ListeningReason enum. */
            enum ListeningReason {

                /** LISTENING_REASON_UNKNOWN value */
                LISTENING_REASON_UNKNOWN = 0,

                /** LISTENING_REASON_USER_REQUEST value */
                LISTENING_REASON_USER_REQUEST = 1,

                /** LISTENING_REASON_QUIET_HOURS value */
                LISTENING_REASON_QUIET_HOURS = 2,

                /** LISTENING_REASON_BLOCKLIST value */
                LISTENING_REASON_BLOCKLIST = 3,

                /** LISTENING_REASON_STOP_WORD value */
                LISTENING_REASON_STOP_WORD = 4,

                /** LISTENING_REASON_PERMISSION_DENIED value */
                LISTENING_REASON_PERMISSION_DENIED = 5,

                /** LISTENING_REASON_DEVICE_CONFLICT value */
                LISTENING_REASON_DEVICE_CONFLICT = 6,

                /** LISTENING_REASON_DEVICE_DISCONNECTED value */
                LISTENING_REASON_DEVICE_DISCONNECTED = 7,

                /** LISTENING_REASON_ENGINE_UNAVAILABLE value */
                LISTENING_REASON_ENGINE_UNAVAILABLE = 8,

                /** LISTENING_REASON_ENGINE_DEGRADED value */
                LISTENING_REASON_ENGINE_DEGRADED = 9,

                /** LISTENING_REASON_SYSTEM_SLEEP value */
                LISTENING_REASON_SYSTEM_SLEEP = 10,

                /** LISTENING_REASON_STORAGE_FAILED value */
                LISTENING_REASON_STORAGE_FAILED = 11
            }

            /** ExtractionState enum. */
            enum ExtractionState {

                /** EXTRACTION_STATE_UNKNOWN value */
                EXTRACTION_STATE_UNKNOWN = 0,

                /** EXTRACTION_STATE_DISABLED value */
                EXTRACTION_STATE_DISABLED = 1,

                /** EXTRACTION_STATE_PENDING value */
                EXTRACTION_STATE_PENDING = 2,

                /** EXTRACTION_STATE_DONE value */
                EXTRACTION_STATE_DONE = 3,

                /** EXTRACTION_STATE_FAILED value */
                EXTRACTION_STATE_FAILED = 4
            }

            /**
             * Properties of an AmbientDevice.
             * @deprecated Use evohime.desktop.v1.AmbientDevice.$Properties instead.
             */
            interface IAmbientDevice extends evohime.desktop.v1.AmbientDevice.$Properties {
            }

            /** Represents an AmbientDevice. */
            class AmbientDevice {

                /**
                 * Constructs a new AmbientDevice.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.AmbientDevice.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** AmbientDevice deviceId. */
                deviceId: string;

                /** AmbientDevice displayName. */
                displayName: string;

                /** AmbientDevice isDefault. */
                isDefault: boolean;

                /** AmbientDevice isActive. */
                isActive: boolean;

                /**
                 * Encodes the specified AmbientDevice message. Does not implicitly {@link evohime.desktop.v1.AmbientDevice.verify|verify} messages.
                 * @param message AmbientDevice message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.AmbientDevice.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an AmbientDevice message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.AmbientDevice & evohime.desktop.v1.AmbientDevice.$Shape} AmbientDevice
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.AmbientDevice & evohime.desktop.v1.AmbientDevice.$Shape;

                /**
                 * Gets the type url for AmbientDevice
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace AmbientDevice {

                /** Properties of an AmbientDevice. */
                interface $Properties {

                    /** AmbientDevice deviceId */
                    deviceId?: (string|null);

                    /** AmbientDevice displayName */
                    displayName?: (string|null);

                    /** AmbientDevice isDefault */
                    isDefault?: (boolean|null);

                    /** AmbientDevice isActive */
                    isActive?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an AmbientDevice. */
                type $Shape = evohime.desktop.v1.AmbientDevice.$Properties;
            }

            /**
             * Properties of an AmbientEpisodeSummary.
             * @deprecated Use evohime.desktop.v1.AmbientEpisodeSummary.$Properties instead.
             */
            interface IAmbientEpisodeSummary extends evohime.desktop.v1.AmbientEpisodeSummary.$Properties {
            }

            /** Represents an AmbientEpisodeSummary. */
            class AmbientEpisodeSummary {

                /**
                 * Constructs a new AmbientEpisodeSummary.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.AmbientEpisodeSummary.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** AmbientEpisodeSummary episodeId. */
                episodeId: string;

                /** AmbientEpisodeSummary startedAtMs. */
                startedAtMs: number;

                /** AmbientEpisodeSummary speechDurationMs. */
                speechDurationMs: number;

                /** AmbientEpisodeSummary utteranceCount. */
                utteranceCount: number;

                /** AmbientEpisodeSummary extractionState. */
                extractionState: evohime.desktop.v1.ExtractionState;

                /**
                 * Encodes the specified AmbientEpisodeSummary message. Does not implicitly {@link evohime.desktop.v1.AmbientEpisodeSummary.verify|verify} messages.
                 * @param message AmbientEpisodeSummary message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.AmbientEpisodeSummary.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an AmbientEpisodeSummary message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.AmbientEpisodeSummary & evohime.desktop.v1.AmbientEpisodeSummary.$Shape} AmbientEpisodeSummary
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.AmbientEpisodeSummary & evohime.desktop.v1.AmbientEpisodeSummary.$Shape;

                /**
                 * Gets the type url for AmbientEpisodeSummary
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace AmbientEpisodeSummary {

                /** Properties of an AmbientEpisodeSummary. */
                interface $Properties {

                    /** AmbientEpisodeSummary episodeId */
                    episodeId?: (string|null);

                    /** AmbientEpisodeSummary startedAtMs */
                    startedAtMs?: (number|null);

                    /** AmbientEpisodeSummary speechDurationMs */
                    speechDurationMs?: (number|null);

                    /** AmbientEpisodeSummary utteranceCount */
                    utteranceCount?: (number|null);

                    /** AmbientEpisodeSummary extractionState */
                    extractionState?: (evohime.desktop.v1.ExtractionState|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an AmbientEpisodeSummary. */
                type $Shape = evohime.desktop.v1.AmbientEpisodeSummary.$Properties;
            }

            /**
             * Properties of an Utterance.
             * @deprecated Use evohime.desktop.v1.Utterance.$Properties instead.
             */
            interface IUtterance extends evohime.desktop.v1.Utterance.$Properties {
            }

            /** Represents an Utterance. */
            class Utterance {

                /**
                 * Constructs a new Utterance.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.Utterance.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** Utterance utteranceId. */
                utteranceId: string;

                /** Utterance startedAtMs. */
                startedAtMs: number;

                /** Utterance durationMs. */
                durationMs: number;

                /** Utterance text. */
                text: string;

                /** Utterance language. */
                language: string;

                /** Utterance redacted. */
                redacted: boolean;

                /**
                 * Encodes the specified Utterance message. Does not implicitly {@link evohime.desktop.v1.Utterance.verify|verify} messages.
                 * @param message Utterance message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.Utterance.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an Utterance message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.Utterance & evohime.desktop.v1.Utterance.$Shape} Utterance
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.Utterance & evohime.desktop.v1.Utterance.$Shape;

                /**
                 * Gets the type url for Utterance
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace Utterance {

                /** Properties of an Utterance. */
                interface $Properties {

                    /** Utterance utteranceId */
                    utteranceId?: (string|null);

                    /** Utterance startedAtMs */
                    startedAtMs?: (number|null);

                    /** Utterance durationMs */
                    durationMs?: (number|null);

                    /** Utterance text */
                    text?: (string|null);

                    /** Utterance language */
                    language?: (string|null);

                    /** Utterance redacted */
                    redacted?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an Utterance. */
                type $Shape = evohime.desktop.v1.Utterance.$Properties;
            }

            /**
             * Properties of a QuietHours.
             * @deprecated Use evohime.desktop.v1.QuietHours.$Properties instead.
             */
            interface IQuietHours extends evohime.desktop.v1.QuietHours.$Properties {
            }

            /** Represents a QuietHours. */
            class QuietHours {

                /**
                 * Constructs a new QuietHours.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.QuietHours.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** QuietHours startMinute. */
                startMinute: number;

                /** QuietHours endMinute. */
                endMinute: number;

                /**
                 * Encodes the specified QuietHours message. Does not implicitly {@link evohime.desktop.v1.QuietHours.verify|verify} messages.
                 * @param message QuietHours message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.QuietHours.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a QuietHours message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.QuietHours & evohime.desktop.v1.QuietHours.$Shape} QuietHours
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.QuietHours & evohime.desktop.v1.QuietHours.$Shape;

                /**
                 * Gets the type url for QuietHours
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace QuietHours {

                /** Properties of a QuietHours. */
                interface $Properties {

                    /** QuietHours startMinute */
                    startMinute?: (number|null);

                    /** QuietHours endMinute */
                    endMinute?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a QuietHours. */
                type $Shape = evohime.desktop.v1.QuietHours.$Properties;
            }

            /**
             * Properties of an AmbientPolicy.
             * @deprecated Use evohime.desktop.v1.AmbientPolicy.$Properties instead.
             */
            interface IAmbientPolicy extends evohime.desktop.v1.AmbientPolicy.$Properties {
            }

            /** Represents an AmbientPolicy. */
            class AmbientPolicy {

                /**
                 * Constructs a new AmbientPolicy.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.AmbientPolicy.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** AmbientPolicy quietHours. */
                quietHours: evohime.desktop.v1.QuietHours.$Properties[];

                /** AmbientPolicy blocklistPatterns. */
                blocklistPatterns: string[];

                /** AmbientPolicy retentionDays. */
                retentionDays: number;

                /** AmbientPolicy windowTitleBlocklist. */
                windowTitleBlocklist: string[];

                /**
                 * Encodes the specified AmbientPolicy message. Does not implicitly {@link evohime.desktop.v1.AmbientPolicy.verify|verify} messages.
                 * @param message AmbientPolicy message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.AmbientPolicy.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an AmbientPolicy message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.AmbientPolicy & evohime.desktop.v1.AmbientPolicy.$Shape} AmbientPolicy
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.AmbientPolicy & evohime.desktop.v1.AmbientPolicy.$Shape;

                /**
                 * Gets the type url for AmbientPolicy
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace AmbientPolicy {

                /** Properties of an AmbientPolicy. */
                interface $Properties {

                    /** AmbientPolicy quietHours */
                    quietHours?: (evohime.desktop.v1.QuietHours.$Properties[]|null);

                    /** AmbientPolicy blocklistPatterns */
                    blocklistPatterns?: (string[]|null);

                    /** AmbientPolicy retentionDays */
                    retentionDays?: (number|null);

                    /** AmbientPolicy windowTitleBlocklist */
                    windowTitleBlocklist?: (string[]|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an AmbientPolicy. */
                type $Shape = evohime.desktop.v1.AmbientPolicy.$Properties;
            }

            /**
             * Properties of a SetAmbientListening.
             * @deprecated Use evohime.desktop.v1.SetAmbientListening.$Properties instead.
             */
            interface ISetAmbientListening extends evohime.desktop.v1.SetAmbientListening.$Properties {
            }

            /** Represents a SetAmbientListening. */
            class SetAmbientListening {

                /**
                 * Constructs a new SetAmbientListening.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SetAmbientListening.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SetAmbientListening enabled. */
                enabled: boolean;

                /** SetAmbientListening paused. */
                paused: boolean;

                /** SetAmbientListening deviceId. */
                deviceId: string;

                /**
                 * Encodes the specified SetAmbientListening message. Does not implicitly {@link evohime.desktop.v1.SetAmbientListening.verify|verify} messages.
                 * @param message SetAmbientListening message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SetAmbientListening.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SetAmbientListening message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SetAmbientListening & evohime.desktop.v1.SetAmbientListening.$Shape} SetAmbientListening
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SetAmbientListening & evohime.desktop.v1.SetAmbientListening.$Shape;

                /**
                 * Gets the type url for SetAmbientListening
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SetAmbientListening {

                /** Properties of a SetAmbientListening. */
                interface $Properties {

                    /** SetAmbientListening enabled */
                    enabled?: (boolean|null);

                    /** SetAmbientListening paused */
                    paused?: (boolean|null);

                    /** SetAmbientListening deviceId */
                    deviceId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SetAmbientListening. */
                type $Shape = evohime.desktop.v1.SetAmbientListening.$Properties;
            }

            /**
             * Properties of a SetAmbientListeningResult.
             * @deprecated Use evohime.desktop.v1.SetAmbientListeningResult.$Properties instead.
             */
            interface ISetAmbientListeningResult extends evohime.desktop.v1.SetAmbientListeningResult.$Properties {
            }

            /** Represents a SetAmbientListeningResult. */
            class SetAmbientListeningResult {

                /**
                 * Constructs a new SetAmbientListeningResult.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SetAmbientListeningResult.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SetAmbientListeningResult state. */
                state: evohime.desktop.v1.ListeningState;

                /** SetAmbientListeningResult errorCode. */
                errorCode: string;

                /**
                 * Encodes the specified SetAmbientListeningResult message. Does not implicitly {@link evohime.desktop.v1.SetAmbientListeningResult.verify|verify} messages.
                 * @param message SetAmbientListeningResult message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SetAmbientListeningResult.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SetAmbientListeningResult message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SetAmbientListeningResult & evohime.desktop.v1.SetAmbientListeningResult.$Shape} SetAmbientListeningResult
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SetAmbientListeningResult & evohime.desktop.v1.SetAmbientListeningResult.$Shape;

                /**
                 * Gets the type url for SetAmbientListeningResult
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SetAmbientListeningResult {

                /** Properties of a SetAmbientListeningResult. */
                interface $Properties {

                    /** SetAmbientListeningResult state */
                    state?: (evohime.desktop.v1.ListeningState|null);

                    /** SetAmbientListeningResult errorCode */
                    errorCode?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SetAmbientListeningResult. */
                type $Shape = evohime.desktop.v1.SetAmbientListeningResult.$Properties;
            }

            /**
             * Properties of a GetAmbientStatus.
             * @deprecated Use evohime.desktop.v1.GetAmbientStatus.$Properties instead.
             */
            interface IGetAmbientStatus extends evohime.desktop.v1.GetAmbientStatus.$Properties {
            }

            /** Represents a GetAmbientStatus. */
            class GetAmbientStatus {

                /**
                 * Constructs a new GetAmbientStatus.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetAmbientStatus.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /**
                 * Encodes the specified GetAmbientStatus message. Does not implicitly {@link evohime.desktop.v1.GetAmbientStatus.verify|verify} messages.
                 * @param message GetAmbientStatus message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetAmbientStatus.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetAmbientStatus message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetAmbientStatus & evohime.desktop.v1.GetAmbientStatus.$Shape} GetAmbientStatus
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetAmbientStatus & evohime.desktop.v1.GetAmbientStatus.$Shape;

                /**
                 * Gets the type url for GetAmbientStatus
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetAmbientStatus {

                /** Properties of a GetAmbientStatus. */
                interface $Properties {

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetAmbientStatus. */
                type $Shape = evohime.desktop.v1.GetAmbientStatus.$Properties;
            }

            /**
             * Properties of an AmbientStatus.
             * @deprecated Use evohime.desktop.v1.AmbientStatus.$Properties instead.
             */
            interface IAmbientStatus extends evohime.desktop.v1.AmbientStatus.$Properties {
            }

            /** Represents an AmbientStatus. */
            class AmbientStatus {

                /**
                 * Constructs a new AmbientStatus.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.AmbientStatus.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** AmbientStatus state. */
                state: evohime.desktop.v1.ListeningState;

                /** AmbientStatus reason. */
                reason: evohime.desktop.v1.ListeningReason;

                /** AmbientStatus activeDeviceId. */
                activeDeviceId: string;

                /** AmbientStatus engineVersion. */
                engineVersion: string;

                /** AmbientStatus engineReady. */
                engineReady: boolean;

                /** AmbientStatus devices. */
                devices: evohime.desktop.v1.AmbientDevice.$Properties[];

                /**
                 * Encodes the specified AmbientStatus message. Does not implicitly {@link evohime.desktop.v1.AmbientStatus.verify|verify} messages.
                 * @param message AmbientStatus message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.AmbientStatus.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an AmbientStatus message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.AmbientStatus & evohime.desktop.v1.AmbientStatus.$Shape} AmbientStatus
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.AmbientStatus & evohime.desktop.v1.AmbientStatus.$Shape;

                /**
                 * Gets the type url for AmbientStatus
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace AmbientStatus {

                /** Properties of an AmbientStatus. */
                interface $Properties {

                    /** AmbientStatus state */
                    state?: (evohime.desktop.v1.ListeningState|null);

                    /** AmbientStatus reason */
                    reason?: (evohime.desktop.v1.ListeningReason|null);

                    /** AmbientStatus activeDeviceId */
                    activeDeviceId?: (string|null);

                    /** AmbientStatus engineVersion */
                    engineVersion?: (string|null);

                    /** AmbientStatus engineReady */
                    engineReady?: (boolean|null);

                    /** AmbientStatus devices */
                    devices?: (evohime.desktop.v1.AmbientDevice.$Properties[]|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an AmbientStatus. */
                type $Shape = evohime.desktop.v1.AmbientStatus.$Properties;
            }

            /**
             * Properties of a ListAmbientEpisodes.
             * @deprecated Use evohime.desktop.v1.ListAmbientEpisodes.$Properties instead.
             */
            interface IListAmbientEpisodes extends evohime.desktop.v1.ListAmbientEpisodes.$Properties {
            }

            /** Represents a ListAmbientEpisodes. */
            class ListAmbientEpisodes {

                /**
                 * Constructs a new ListAmbientEpisodes.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ListAmbientEpisodes.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ListAmbientEpisodes sinceMs. */
                sinceMs: number;

                /** ListAmbientEpisodes limit. */
                limit: number;

                /** ListAmbientEpisodes cursor. */
                cursor: string;

                /**
                 * Encodes the specified ListAmbientEpisodes message. Does not implicitly {@link evohime.desktop.v1.ListAmbientEpisodes.verify|verify} messages.
                 * @param message ListAmbientEpisodes message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ListAmbientEpisodes.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ListAmbientEpisodes message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListAmbientEpisodes & evohime.desktop.v1.ListAmbientEpisodes.$Shape} ListAmbientEpisodes
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ListAmbientEpisodes & evohime.desktop.v1.ListAmbientEpisodes.$Shape;

                /**
                 * Gets the type url for ListAmbientEpisodes
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ListAmbientEpisodes {

                /** Properties of a ListAmbientEpisodes. */
                interface $Properties {

                    /** ListAmbientEpisodes sinceMs */
                    sinceMs?: (number|null);

                    /** ListAmbientEpisodes limit */
                    limit?: (number|null);

                    /** ListAmbientEpisodes cursor */
                    cursor?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ListAmbientEpisodes. */
                type $Shape = evohime.desktop.v1.ListAmbientEpisodes.$Properties;
            }

            /**
             * Properties of an AmbientEpisodeList.
             * @deprecated Use evohime.desktop.v1.AmbientEpisodeList.$Properties instead.
             */
            interface IAmbientEpisodeList extends evohime.desktop.v1.AmbientEpisodeList.$Properties {
            }

            /** Represents an AmbientEpisodeList. */
            class AmbientEpisodeList {

                /**
                 * Constructs a new AmbientEpisodeList.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.AmbientEpisodeList.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** AmbientEpisodeList episodes. */
                episodes: evohime.desktop.v1.AmbientEpisodeSummary.$Properties[];

                /** AmbientEpisodeList nextCursor. */
                nextCursor: string;

                /**
                 * Encodes the specified AmbientEpisodeList message. Does not implicitly {@link evohime.desktop.v1.AmbientEpisodeList.verify|verify} messages.
                 * @param message AmbientEpisodeList message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.AmbientEpisodeList.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an AmbientEpisodeList message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.AmbientEpisodeList & evohime.desktop.v1.AmbientEpisodeList.$Shape} AmbientEpisodeList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.AmbientEpisodeList & evohime.desktop.v1.AmbientEpisodeList.$Shape;

                /**
                 * Gets the type url for AmbientEpisodeList
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace AmbientEpisodeList {

                /** Properties of an AmbientEpisodeList. */
                interface $Properties {

                    /** AmbientEpisodeList episodes */
                    episodes?: (evohime.desktop.v1.AmbientEpisodeSummary.$Properties[]|null);

                    /** AmbientEpisodeList nextCursor */
                    nextCursor?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an AmbientEpisodeList. */
                type $Shape = evohime.desktop.v1.AmbientEpisodeList.$Properties;
            }

            /**
             * Properties of a GetAmbientEpisode.
             * @deprecated Use evohime.desktop.v1.GetAmbientEpisode.$Properties instead.
             */
            interface IGetAmbientEpisode extends evohime.desktop.v1.GetAmbientEpisode.$Properties {
            }

            /** Represents a GetAmbientEpisode. */
            class GetAmbientEpisode {

                /**
                 * Constructs a new GetAmbientEpisode.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetAmbientEpisode.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GetAmbientEpisode episodeId. */
                episodeId: string;

                /**
                 * Encodes the specified GetAmbientEpisode message. Does not implicitly {@link evohime.desktop.v1.GetAmbientEpisode.verify|verify} messages.
                 * @param message GetAmbientEpisode message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetAmbientEpisode.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetAmbientEpisode message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetAmbientEpisode & evohime.desktop.v1.GetAmbientEpisode.$Shape} GetAmbientEpisode
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetAmbientEpisode & evohime.desktop.v1.GetAmbientEpisode.$Shape;

                /**
                 * Gets the type url for GetAmbientEpisode
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetAmbientEpisode {

                /** Properties of a GetAmbientEpisode. */
                interface $Properties {

                    /** GetAmbientEpisode episodeId */
                    episodeId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetAmbientEpisode. */
                type $Shape = evohime.desktop.v1.GetAmbientEpisode.$Properties;
            }

            /**
             * Properties of an AmbientEpisodeDetail.
             * @deprecated Use evohime.desktop.v1.AmbientEpisodeDetail.$Properties instead.
             */
            interface IAmbientEpisodeDetail extends evohime.desktop.v1.AmbientEpisodeDetail.$Properties {
            }

            /** Represents an AmbientEpisodeDetail. */
            class AmbientEpisodeDetail {

                /**
                 * Constructs a new AmbientEpisodeDetail.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.AmbientEpisodeDetail.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** AmbientEpisodeDetail episodeId. */
                episodeId: string;

                /** AmbientEpisodeDetail utterances. */
                utterances: evohime.desktop.v1.Utterance.$Properties[];

                /**
                 * Encodes the specified AmbientEpisodeDetail message. Does not implicitly {@link evohime.desktop.v1.AmbientEpisodeDetail.verify|verify} messages.
                 * @param message AmbientEpisodeDetail message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.AmbientEpisodeDetail.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an AmbientEpisodeDetail message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.AmbientEpisodeDetail & evohime.desktop.v1.AmbientEpisodeDetail.$Shape} AmbientEpisodeDetail
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.AmbientEpisodeDetail & evohime.desktop.v1.AmbientEpisodeDetail.$Shape;

                /**
                 * Gets the type url for AmbientEpisodeDetail
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace AmbientEpisodeDetail {

                /** Properties of an AmbientEpisodeDetail. */
                interface $Properties {

                    /** AmbientEpisodeDetail episodeId */
                    episodeId?: (string|null);

                    /** AmbientEpisodeDetail utterances */
                    utterances?: (evohime.desktop.v1.Utterance.$Properties[]|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an AmbientEpisodeDetail. */
                type $Shape = evohime.desktop.v1.AmbientEpisodeDetail.$Properties;
            }

            /**
             * Properties of a DeleteAmbientTranscripts.
             * @deprecated Use evohime.desktop.v1.DeleteAmbientTranscripts.$Properties instead.
             */
            interface IDeleteAmbientTranscripts extends evohime.desktop.v1.DeleteAmbientTranscripts.$Properties {
            }

            /** Represents a DeleteAmbientTranscripts. */
            class DeleteAmbientTranscripts {

                /**
                 * Constructs a new DeleteAmbientTranscripts.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.DeleteAmbientTranscripts.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** DeleteAmbientTranscripts episodeIds. */
                episodeIds: string[];

                /** DeleteAmbientTranscripts all. */
                all: boolean;

                /** DeleteAmbientTranscripts confirmed. */
                confirmed: boolean;

                /**
                 * Encodes the specified DeleteAmbientTranscripts message. Does not implicitly {@link evohime.desktop.v1.DeleteAmbientTranscripts.verify|verify} messages.
                 * @param message DeleteAmbientTranscripts message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.DeleteAmbientTranscripts.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a DeleteAmbientTranscripts message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.DeleteAmbientTranscripts & evohime.desktop.v1.DeleteAmbientTranscripts.$Shape} DeleteAmbientTranscripts
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.DeleteAmbientTranscripts & evohime.desktop.v1.DeleteAmbientTranscripts.$Shape;

                /**
                 * Gets the type url for DeleteAmbientTranscripts
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace DeleteAmbientTranscripts {

                /** Properties of a DeleteAmbientTranscripts. */
                interface $Properties {

                    /** DeleteAmbientTranscripts episodeIds */
                    episodeIds?: (string[]|null);

                    /** DeleteAmbientTranscripts all */
                    all?: (boolean|null);

                    /** DeleteAmbientTranscripts confirmed */
                    confirmed?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a DeleteAmbientTranscripts. */
                type $Shape = evohime.desktop.v1.DeleteAmbientTranscripts.$Properties;
            }

            /**
             * Properties of a DeleteAmbientTranscriptsResult.
             * @deprecated Use evohime.desktop.v1.DeleteAmbientTranscriptsResult.$Properties instead.
             */
            interface IDeleteAmbientTranscriptsResult extends evohime.desktop.v1.DeleteAmbientTranscriptsResult.$Properties {
            }

            /** Represents a DeleteAmbientTranscriptsResult. */
            class DeleteAmbientTranscriptsResult {

                /**
                 * Constructs a new DeleteAmbientTranscriptsResult.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.DeleteAmbientTranscriptsResult.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** DeleteAmbientTranscriptsResult deletedCount. */
                deletedCount: number;

                /**
                 * Encodes the specified DeleteAmbientTranscriptsResult message. Does not implicitly {@link evohime.desktop.v1.DeleteAmbientTranscriptsResult.verify|verify} messages.
                 * @param message DeleteAmbientTranscriptsResult message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.DeleteAmbientTranscriptsResult.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a DeleteAmbientTranscriptsResult message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.DeleteAmbientTranscriptsResult & evohime.desktop.v1.DeleteAmbientTranscriptsResult.$Shape} DeleteAmbientTranscriptsResult
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.DeleteAmbientTranscriptsResult & evohime.desktop.v1.DeleteAmbientTranscriptsResult.$Shape;

                /**
                 * Gets the type url for DeleteAmbientTranscriptsResult
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace DeleteAmbientTranscriptsResult {

                /** Properties of a DeleteAmbientTranscriptsResult. */
                interface $Properties {

                    /** DeleteAmbientTranscriptsResult deletedCount */
                    deletedCount?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a DeleteAmbientTranscriptsResult. */
                type $Shape = evohime.desktop.v1.DeleteAmbientTranscriptsResult.$Properties;
            }

            /**
             * Properties of a ForgetAmbientWindow.
             * @deprecated Use evohime.desktop.v1.ForgetAmbientWindow.$Properties instead.
             */
            interface IForgetAmbientWindow extends evohime.desktop.v1.ForgetAmbientWindow.$Properties {
            }

            /** Represents a ForgetAmbientWindow. */
            class ForgetAmbientWindow {

                /**
                 * Constructs a new ForgetAmbientWindow.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ForgetAmbientWindow.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ForgetAmbientWindow windowMs. */
                windowMs: number;

                /** ForgetAmbientWindow confirmed. */
                confirmed: boolean;

                /**
                 * Encodes the specified ForgetAmbientWindow message. Does not implicitly {@link evohime.desktop.v1.ForgetAmbientWindow.verify|verify} messages.
                 * @param message ForgetAmbientWindow message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ForgetAmbientWindow.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ForgetAmbientWindow message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ForgetAmbientWindow & evohime.desktop.v1.ForgetAmbientWindow.$Shape} ForgetAmbientWindow
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ForgetAmbientWindow & evohime.desktop.v1.ForgetAmbientWindow.$Shape;

                /**
                 * Gets the type url for ForgetAmbientWindow
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ForgetAmbientWindow {

                /** Properties of a ForgetAmbientWindow. */
                interface $Properties {

                    /** ForgetAmbientWindow windowMs */
                    windowMs?: (number|null);

                    /** ForgetAmbientWindow confirmed */
                    confirmed?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ForgetAmbientWindow. */
                type $Shape = evohime.desktop.v1.ForgetAmbientWindow.$Properties;
            }

            /**
             * Properties of a ForgetAmbientWindowResult.
             * @deprecated Use evohime.desktop.v1.ForgetAmbientWindowResult.$Properties instead.
             */
            interface IForgetAmbientWindowResult extends evohime.desktop.v1.ForgetAmbientWindowResult.$Properties {
            }

            /** Represents a ForgetAmbientWindowResult. */
            class ForgetAmbientWindowResult {

                /**
                 * Constructs a new ForgetAmbientWindowResult.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ForgetAmbientWindowResult.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ForgetAmbientWindowResult deletedCount. */
                deletedCount: number;

                /**
                 * Encodes the specified ForgetAmbientWindowResult message. Does not implicitly {@link evohime.desktop.v1.ForgetAmbientWindowResult.verify|verify} messages.
                 * @param message ForgetAmbientWindowResult message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ForgetAmbientWindowResult.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ForgetAmbientWindowResult message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ForgetAmbientWindowResult & evohime.desktop.v1.ForgetAmbientWindowResult.$Shape} ForgetAmbientWindowResult
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ForgetAmbientWindowResult & evohime.desktop.v1.ForgetAmbientWindowResult.$Shape;

                /**
                 * Gets the type url for ForgetAmbientWindowResult
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ForgetAmbientWindowResult {

                /** Properties of a ForgetAmbientWindowResult. */
                interface $Properties {

                    /** ForgetAmbientWindowResult deletedCount */
                    deletedCount?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ForgetAmbientWindowResult. */
                type $Shape = evohime.desktop.v1.ForgetAmbientWindowResult.$Properties;
            }

            /**
             * Properties of a GetAmbientPolicy.
             * @deprecated Use evohime.desktop.v1.GetAmbientPolicy.$Properties instead.
             */
            interface IGetAmbientPolicy extends evohime.desktop.v1.GetAmbientPolicy.$Properties {
            }

            /** Represents a GetAmbientPolicy. */
            class GetAmbientPolicy {

                /**
                 * Constructs a new GetAmbientPolicy.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetAmbientPolicy.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /**
                 * Encodes the specified GetAmbientPolicy message. Does not implicitly {@link evohime.desktop.v1.GetAmbientPolicy.verify|verify} messages.
                 * @param message GetAmbientPolicy message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetAmbientPolicy.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetAmbientPolicy message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetAmbientPolicy & evohime.desktop.v1.GetAmbientPolicy.$Shape} GetAmbientPolicy
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetAmbientPolicy & evohime.desktop.v1.GetAmbientPolicy.$Shape;

                /**
                 * Gets the type url for GetAmbientPolicy
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetAmbientPolicy {

                /** Properties of a GetAmbientPolicy. */
                interface $Properties {

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetAmbientPolicy. */
                type $Shape = evohime.desktop.v1.GetAmbientPolicy.$Properties;
            }

            /**
             * Properties of a SaveAmbientPolicy.
             * @deprecated Use evohime.desktop.v1.SaveAmbientPolicy.$Properties instead.
             */
            interface ISaveAmbientPolicy extends evohime.desktop.v1.SaveAmbientPolicy.$Properties {
            }

            /** Represents a SaveAmbientPolicy. */
            class SaveAmbientPolicy {

                /**
                 * Constructs a new SaveAmbientPolicy.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SaveAmbientPolicy.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SaveAmbientPolicy policy. */
                policy?: (evohime.desktop.v1.AmbientPolicy.$Properties|null);

                /**
                 * Encodes the specified SaveAmbientPolicy message. Does not implicitly {@link evohime.desktop.v1.SaveAmbientPolicy.verify|verify} messages.
                 * @param message SaveAmbientPolicy message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SaveAmbientPolicy.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SaveAmbientPolicy message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SaveAmbientPolicy & evohime.desktop.v1.SaveAmbientPolicy.$Shape} SaveAmbientPolicy
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SaveAmbientPolicy & evohime.desktop.v1.SaveAmbientPolicy.$Shape;

                /**
                 * Gets the type url for SaveAmbientPolicy
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SaveAmbientPolicy {

                /** Properties of a SaveAmbientPolicy. */
                interface $Properties {

                    /** SaveAmbientPolicy policy */
                    policy?: (evohime.desktop.v1.AmbientPolicy.$Properties|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SaveAmbientPolicy. */
                type $Shape = evohime.desktop.v1.SaveAmbientPolicy.$Properties;
            }

            /**
             * Properties of a SaveAmbientPolicyResult.
             * @deprecated Use evohime.desktop.v1.SaveAmbientPolicyResult.$Properties instead.
             */
            interface ISaveAmbientPolicyResult extends evohime.desktop.v1.SaveAmbientPolicyResult.$Properties {
            }

            /** Represents a SaveAmbientPolicyResult. */
            class SaveAmbientPolicyResult {

                /**
                 * Constructs a new SaveAmbientPolicyResult.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.SaveAmbientPolicyResult.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** SaveAmbientPolicyResult applied. */
                applied: boolean;

                /** SaveAmbientPolicyResult errorCode. */
                errorCode: string;

                /**
                 * Encodes the specified SaveAmbientPolicyResult message. Does not implicitly {@link evohime.desktop.v1.SaveAmbientPolicyResult.verify|verify} messages.
                 * @param message SaveAmbientPolicyResult message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.SaveAmbientPolicyResult.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a SaveAmbientPolicyResult message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.SaveAmbientPolicyResult & evohime.desktop.v1.SaveAmbientPolicyResult.$Shape} SaveAmbientPolicyResult
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.SaveAmbientPolicyResult & evohime.desktop.v1.SaveAmbientPolicyResult.$Shape;

                /**
                 * Gets the type url for SaveAmbientPolicyResult
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace SaveAmbientPolicyResult {

                /** Properties of a SaveAmbientPolicyResult. */
                interface $Properties {

                    /** SaveAmbientPolicyResult applied */
                    applied?: (boolean|null);

                    /** SaveAmbientPolicyResult errorCode */
                    errorCode?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a SaveAmbientPolicyResult. */
                type $Shape = evohime.desktop.v1.SaveAmbientPolicyResult.$Properties;
            }

            /**
             * Properties of a ResolveAmbientProposal.
             * @deprecated Use evohime.desktop.v1.ResolveAmbientProposal.$Properties instead.
             */
            interface IResolveAmbientProposal extends evohime.desktop.v1.ResolveAmbientProposal.$Properties {
            }

            /** Represents a ResolveAmbientProposal. */
            class ResolveAmbientProposal {

                /**
                 * Constructs a new ResolveAmbientProposal.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ResolveAmbientProposal.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ResolveAmbientProposal proposalId. */
                proposalId: string;

                /** ResolveAmbientProposal accepted. */
                accepted: boolean;

                /** ResolveAmbientProposal idempotencyKey. */
                idempotencyKey: string;

                /** ResolveAmbientProposal mute. */
                mute: boolean;

                /**
                 * Encodes the specified ResolveAmbientProposal message. Does not implicitly {@link evohime.desktop.v1.ResolveAmbientProposal.verify|verify} messages.
                 * @param message ResolveAmbientProposal message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ResolveAmbientProposal.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ResolveAmbientProposal message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ResolveAmbientProposal & evohime.desktop.v1.ResolveAmbientProposal.$Shape} ResolveAmbientProposal
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ResolveAmbientProposal & evohime.desktop.v1.ResolveAmbientProposal.$Shape;

                /**
                 * Gets the type url for ResolveAmbientProposal
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ResolveAmbientProposal {

                /** Properties of a ResolveAmbientProposal. */
                interface $Properties {

                    /** ResolveAmbientProposal proposalId */
                    proposalId?: (string|null);

                    /** ResolveAmbientProposal accepted */
                    accepted?: (boolean|null);

                    /** ResolveAmbientProposal idempotencyKey */
                    idempotencyKey?: (string|null);

                    /** ResolveAmbientProposal mute */
                    mute?: (boolean|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ResolveAmbientProposal. */
                type $Shape = evohime.desktop.v1.ResolveAmbientProposal.$Properties;
            }

            /**
             * Properties of a ResolveAmbientProposalResult.
             * @deprecated Use evohime.desktop.v1.ResolveAmbientProposalResult.$Properties instead.
             */
            interface IResolveAmbientProposalResult extends evohime.desktop.v1.ResolveAmbientProposalResult.$Properties {
            }

            /** Represents a ResolveAmbientProposalResult. */
            class ResolveAmbientProposalResult {

                /**
                 * Constructs a new ResolveAmbientProposalResult.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ResolveAmbientProposalResult.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ResolveAmbientProposalResult applied. */
                applied: boolean;

                /** ResolveAmbientProposalResult state. */
                state: string;

                /** ResolveAmbientProposalResult taskId. */
                taskId: string;

                /** ResolveAmbientProposalResult errorCode. */
                errorCode: string;

                /**
                 * Encodes the specified ResolveAmbientProposalResult message. Does not implicitly {@link evohime.desktop.v1.ResolveAmbientProposalResult.verify|verify} messages.
                 * @param message ResolveAmbientProposalResult message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ResolveAmbientProposalResult.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ResolveAmbientProposalResult message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ResolveAmbientProposalResult & evohime.desktop.v1.ResolveAmbientProposalResult.$Shape} ResolveAmbientProposalResult
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ResolveAmbientProposalResult & evohime.desktop.v1.ResolveAmbientProposalResult.$Shape;

                /**
                 * Gets the type url for ResolveAmbientProposalResult
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ResolveAmbientProposalResult {

                /** Properties of a ResolveAmbientProposalResult. */
                interface $Properties {

                    /** ResolveAmbientProposalResult applied */
                    applied?: (boolean|null);

                    /** ResolveAmbientProposalResult state */
                    state?: (string|null);

                    /** ResolveAmbientProposalResult taskId */
                    taskId?: (string|null);

                    /** ResolveAmbientProposalResult errorCode */
                    errorCode?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ResolveAmbientProposalResult. */
                type $Shape = evohime.desktop.v1.ResolveAmbientProposalResult.$Properties;
            }

            /**
             * Properties of an AmbientProposalSummary.
             * @deprecated Use evohime.desktop.v1.AmbientProposalSummary.$Properties instead.
             */
            interface IAmbientProposalSummary extends evohime.desktop.v1.AmbientProposalSummary.$Properties {
            }

            /** Represents an AmbientProposalSummary. */
            class AmbientProposalSummary {

                /**
                 * Constructs a new AmbientProposalSummary.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.AmbientProposalSummary.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** AmbientProposalSummary proposalId. */
                proposalId: string;

                /** AmbientProposalSummary kind. */
                kind: string;

                /** AmbientProposalSummary subject. */
                subject: string;

                /** AmbientProposalSummary title. */
                title: string;

                /** AmbientProposalSummary sourceEpisodeId. */
                sourceEpisodeId: string;

                /** AmbientProposalSummary createdAtMs. */
                createdAtMs: number;

                /** AmbientProposalSummary expiresAtMs. */
                expiresAtMs: number;

                /** AmbientProposalSummary occurrences. */
                occurrences: number;

                /** AmbientProposalSummary state. */
                state: string;

                /**
                 * Encodes the specified AmbientProposalSummary message. Does not implicitly {@link evohime.desktop.v1.AmbientProposalSummary.verify|verify} messages.
                 * @param message AmbientProposalSummary message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.AmbientProposalSummary.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an AmbientProposalSummary message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.AmbientProposalSummary & evohime.desktop.v1.AmbientProposalSummary.$Shape} AmbientProposalSummary
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.AmbientProposalSummary & evohime.desktop.v1.AmbientProposalSummary.$Shape;

                /**
                 * Gets the type url for AmbientProposalSummary
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace AmbientProposalSummary {

                /** Properties of an AmbientProposalSummary. */
                interface $Properties {

                    /** AmbientProposalSummary proposalId */
                    proposalId?: (string|null);

                    /** AmbientProposalSummary kind */
                    kind?: (string|null);

                    /** AmbientProposalSummary subject */
                    subject?: (string|null);

                    /** AmbientProposalSummary title */
                    title?: (string|null);

                    /** AmbientProposalSummary sourceEpisodeId */
                    sourceEpisodeId?: (string|null);

                    /** AmbientProposalSummary createdAtMs */
                    createdAtMs?: (number|null);

                    /** AmbientProposalSummary expiresAtMs */
                    expiresAtMs?: (number|null);

                    /** AmbientProposalSummary occurrences */
                    occurrences?: (number|null);

                    /** AmbientProposalSummary state */
                    state?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an AmbientProposalSummary. */
                type $Shape = evohime.desktop.v1.AmbientProposalSummary.$Properties;
            }

            /**
             * Properties of a ListAmbientProposals.
             * @deprecated Use evohime.desktop.v1.ListAmbientProposals.$Properties instead.
             */
            interface IListAmbientProposals extends evohime.desktop.v1.ListAmbientProposals.$Properties {
            }

            /** Represents a ListAmbientProposals. */
            class ListAmbientProposals {

                /**
                 * Constructs a new ListAmbientProposals.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ListAmbientProposals.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ListAmbientProposals limit. */
                limit: number;

                /**
                 * Encodes the specified ListAmbientProposals message. Does not implicitly {@link evohime.desktop.v1.ListAmbientProposals.verify|verify} messages.
                 * @param message ListAmbientProposals message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ListAmbientProposals.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ListAmbientProposals message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListAmbientProposals & evohime.desktop.v1.ListAmbientProposals.$Shape} ListAmbientProposals
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ListAmbientProposals & evohime.desktop.v1.ListAmbientProposals.$Shape;

                /**
                 * Gets the type url for ListAmbientProposals
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ListAmbientProposals {

                /** Properties of a ListAmbientProposals. */
                interface $Properties {

                    /** ListAmbientProposals limit */
                    limit?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ListAmbientProposals. */
                type $Shape = evohime.desktop.v1.ListAmbientProposals.$Properties;
            }

            /**
             * Properties of an AmbientProposalList.
             * @deprecated Use evohime.desktop.v1.AmbientProposalList.$Properties instead.
             */
            interface IAmbientProposalList extends evohime.desktop.v1.AmbientProposalList.$Properties {
            }

            /** Represents an AmbientProposalList. */
            class AmbientProposalList {

                /**
                 * Constructs a new AmbientProposalList.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.AmbientProposalList.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** AmbientProposalList proposals. */
                proposals: evohime.desktop.v1.AmbientProposalSummary.$Properties[];

                /** AmbientProposalList maxPerHour. */
                maxPerHour: number;

                /** AmbientProposalList maxPerDay. */
                maxPerDay: number;

                /** AmbientProposalList minIntervalMs. */
                minIntervalMs: number;

                /**
                 * Encodes the specified AmbientProposalList message. Does not implicitly {@link evohime.desktop.v1.AmbientProposalList.verify|verify} messages.
                 * @param message AmbientProposalList message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.AmbientProposalList.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an AmbientProposalList message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.AmbientProposalList & evohime.desktop.v1.AmbientProposalList.$Shape} AmbientProposalList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.AmbientProposalList & evohime.desktop.v1.AmbientProposalList.$Shape;

                /**
                 * Gets the type url for AmbientProposalList
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace AmbientProposalList {

                /** Properties of an AmbientProposalList. */
                interface $Properties {

                    /** AmbientProposalList proposals */
                    proposals?: (evohime.desktop.v1.AmbientProposalSummary.$Properties[]|null);

                    /** AmbientProposalList maxPerHour */
                    maxPerHour?: (number|null);

                    /** AmbientProposalList maxPerDay */
                    maxPerDay?: (number|null);

                    /** AmbientProposalList minIntervalMs */
                    minIntervalMs?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of an AmbientProposalList. */
                type $Shape = evohime.desktop.v1.AmbientProposalList.$Properties;
            }

            /**
             * Properties of a ListWorkflowTemplates.
             * @deprecated Use evohime.desktop.v1.ListWorkflowTemplates.$Properties instead.
             */
            interface IListWorkflowTemplates extends evohime.desktop.v1.ListWorkflowTemplates.$Properties {
            }

            /** Represents a ListWorkflowTemplates. */
            class ListWorkflowTemplates {

                /**
                 * Constructs a new ListWorkflowTemplates.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ListWorkflowTemplates.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /**
                 * Encodes the specified ListWorkflowTemplates message. Does not implicitly {@link evohime.desktop.v1.ListWorkflowTemplates.verify|verify} messages.
                 * @param message ListWorkflowTemplates message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ListWorkflowTemplates.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ListWorkflowTemplates message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListWorkflowTemplates & evohime.desktop.v1.ListWorkflowTemplates.$Shape} ListWorkflowTemplates
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ListWorkflowTemplates & evohime.desktop.v1.ListWorkflowTemplates.$Shape;

                /**
                 * Gets the type url for ListWorkflowTemplates
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ListWorkflowTemplates {

                /** Properties of a ListWorkflowTemplates. */
                interface $Properties {

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ListWorkflowTemplates. */
                type $Shape = evohime.desktop.v1.ListWorkflowTemplates.$Properties;
            }

            /**
             * Properties of a GetWorkflowDefinition.
             * @deprecated Use evohime.desktop.v1.GetWorkflowDefinition.$Properties instead.
             */
            interface IGetWorkflowDefinition extends evohime.desktop.v1.GetWorkflowDefinition.$Properties {
            }

            /** Represents a GetWorkflowDefinition. */
            class GetWorkflowDefinition {

                /**
                 * Constructs a new GetWorkflowDefinition.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetWorkflowDefinition.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GetWorkflowDefinition templateId. */
                templateId: string;

                /**
                 * Encodes the specified GetWorkflowDefinition message. Does not implicitly {@link evohime.desktop.v1.GetWorkflowDefinition.verify|verify} messages.
                 * @param message GetWorkflowDefinition message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetWorkflowDefinition.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetWorkflowDefinition message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetWorkflowDefinition & evohime.desktop.v1.GetWorkflowDefinition.$Shape} GetWorkflowDefinition
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetWorkflowDefinition & evohime.desktop.v1.GetWorkflowDefinition.$Shape;

                /**
                 * Gets the type url for GetWorkflowDefinition
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetWorkflowDefinition {

                /** Properties of a GetWorkflowDefinition. */
                interface $Properties {

                    /** GetWorkflowDefinition templateId */
                    templateId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetWorkflowDefinition. */
                type $Shape = evohime.desktop.v1.GetWorkflowDefinition.$Properties;
            }

            /**
             * Properties of a WorkflowInput.
             * @deprecated Use evohime.desktop.v1.WorkflowInput.$Properties instead.
             */
            interface IWorkflowInput extends evohime.desktop.v1.WorkflowInput.$Properties {
            }

            /** Represents a WorkflowInput. */
            class WorkflowInput {

                /**
                 * Constructs a new WorkflowInput.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.WorkflowInput.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** WorkflowInput name. */
                name: string;

                /** WorkflowInput value. */
                value: string;

                /**
                 * Encodes the specified WorkflowInput message. Does not implicitly {@link evohime.desktop.v1.WorkflowInput.verify|verify} messages.
                 * @param message WorkflowInput message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.WorkflowInput.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a WorkflowInput message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.WorkflowInput & evohime.desktop.v1.WorkflowInput.$Shape} WorkflowInput
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.WorkflowInput & evohime.desktop.v1.WorkflowInput.$Shape;

                /**
                 * Gets the type url for WorkflowInput
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace WorkflowInput {

                /** Properties of a WorkflowInput. */
                interface $Properties {

                    /** WorkflowInput name */
                    name?: (string|null);

                    /** WorkflowInput value */
                    value?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a WorkflowInput. */
                type $Shape = evohime.desktop.v1.WorkflowInput.$Properties;
            }

            /**
             * Properties of a StartWorkflow.
             * @deprecated Use evohime.desktop.v1.StartWorkflow.$Properties instead.
             */
            interface IStartWorkflow extends evohime.desktop.v1.StartWorkflow.$Properties {
            }

            /** Represents a StartWorkflow. */
            class StartWorkflow {

                /**
                 * Constructs a new StartWorkflow.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.StartWorkflow.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** StartWorkflow templateId. */
                templateId: string;

                /** StartWorkflow taskId. */
                taskId: string;

                /** StartWorkflow workspacePath. */
                workspacePath: string;

                /** StartWorkflow inputs. */
                inputs: evohime.desktop.v1.WorkflowInput.$Properties[];

                /** StartWorkflow idempotencyKey. */
                idempotencyKey: string;

                /**
                 * Encodes the specified StartWorkflow message. Does not implicitly {@link evohime.desktop.v1.StartWorkflow.verify|verify} messages.
                 * @param message StartWorkflow message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.StartWorkflow.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a StartWorkflow message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.StartWorkflow & evohime.desktop.v1.StartWorkflow.$Shape} StartWorkflow
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.StartWorkflow & evohime.desktop.v1.StartWorkflow.$Shape;

                /**
                 * Gets the type url for StartWorkflow
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace StartWorkflow {

                /** Properties of a StartWorkflow. */
                interface $Properties {

                    /** StartWorkflow templateId */
                    templateId?: (string|null);

                    /** StartWorkflow taskId */
                    taskId?: (string|null);

                    /** StartWorkflow workspacePath */
                    workspacePath?: (string|null);

                    /** StartWorkflow inputs */
                    inputs?: (evohime.desktop.v1.WorkflowInput.$Properties[]|null);

                    /** StartWorkflow idempotencyKey */
                    idempotencyKey?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a StartWorkflow. */
                type $Shape = evohime.desktop.v1.StartWorkflow.$Properties;
            }

            /**
             * Properties of a GetWorkflowRun.
             * @deprecated Use evohime.desktop.v1.GetWorkflowRun.$Properties instead.
             */
            interface IGetWorkflowRun extends evohime.desktop.v1.GetWorkflowRun.$Properties {
            }

            /** Represents a GetWorkflowRun. */
            class GetWorkflowRun {

                /**
                 * Constructs a new GetWorkflowRun.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.GetWorkflowRun.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** GetWorkflowRun runId. */
                runId: string;

                /**
                 * Encodes the specified GetWorkflowRun message. Does not implicitly {@link evohime.desktop.v1.GetWorkflowRun.verify|verify} messages.
                 * @param message GetWorkflowRun message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.GetWorkflowRun.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a GetWorkflowRun message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.GetWorkflowRun & evohime.desktop.v1.GetWorkflowRun.$Shape} GetWorkflowRun
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.GetWorkflowRun & evohime.desktop.v1.GetWorkflowRun.$Shape;

                /**
                 * Gets the type url for GetWorkflowRun
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace GetWorkflowRun {

                /** Properties of a GetWorkflowRun. */
                interface $Properties {

                    /** GetWorkflowRun runId */
                    runId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a GetWorkflowRun. */
                type $Shape = evohime.desktop.v1.GetWorkflowRun.$Properties;
            }

            /**
             * Properties of a CancelWorkflow.
             * @deprecated Use evohime.desktop.v1.CancelWorkflow.$Properties instead.
             */
            interface ICancelWorkflow extends evohime.desktop.v1.CancelWorkflow.$Properties {
            }

            /** Represents a CancelWorkflow. */
            class CancelWorkflow {

                /**
                 * Constructs a new CancelWorkflow.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.CancelWorkflow.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** CancelWorkflow runId. */
                runId: string;

                /**
                 * Encodes the specified CancelWorkflow message. Does not implicitly {@link evohime.desktop.v1.CancelWorkflow.verify|verify} messages.
                 * @param message CancelWorkflow message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.CancelWorkflow.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CancelWorkflow message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.CancelWorkflow & evohime.desktop.v1.CancelWorkflow.$Shape} CancelWorkflow
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.CancelWorkflow & evohime.desktop.v1.CancelWorkflow.$Shape;

                /**
                 * Gets the type url for CancelWorkflow
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace CancelWorkflow {

                /** Properties of a CancelWorkflow. */
                interface $Properties {

                    /** CancelWorkflow runId */
                    runId?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a CancelWorkflow. */
                type $Shape = evohime.desktop.v1.CancelWorkflow.$Properties;
            }

            /**
             * Properties of a ListWorkflowEvents.
             * @deprecated Use evohime.desktop.v1.ListWorkflowEvents.$Properties instead.
             */
            interface IListWorkflowEvents extends evohime.desktop.v1.ListWorkflowEvents.$Properties {
            }

            /** Represents a ListWorkflowEvents. */
            class ListWorkflowEvents {

                /**
                 * Constructs a new ListWorkflowEvents.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ListWorkflowEvents.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ListWorkflowEvents runId. */
                runId: string;

                /** ListWorkflowEvents afterSequence. */
                afterSequence: number;

                /** ListWorkflowEvents limit. */
                limit: number;

                /**
                 * Encodes the specified ListWorkflowEvents message. Does not implicitly {@link evohime.desktop.v1.ListWorkflowEvents.verify|verify} messages.
                 * @param message ListWorkflowEvents message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ListWorkflowEvents.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ListWorkflowEvents message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ListWorkflowEvents & evohime.desktop.v1.ListWorkflowEvents.$Shape} ListWorkflowEvents
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ListWorkflowEvents & evohime.desktop.v1.ListWorkflowEvents.$Shape;

                /**
                 * Gets the type url for ListWorkflowEvents
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ListWorkflowEvents {

                /** Properties of a ListWorkflowEvents. */
                interface $Properties {

                    /** ListWorkflowEvents runId */
                    runId?: (string|null);

                    /** ListWorkflowEvents afterSequence */
                    afterSequence?: (number|null);

                    /** ListWorkflowEvents limit */
                    limit?: (number|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ListWorkflowEvents. */
                type $Shape = evohime.desktop.v1.ListWorkflowEvents.$Properties;
            }

            /**
             * Properties of a CommandEnvelope.
             * @deprecated Use evohime.desktop.v1.CommandEnvelope.$Properties instead.
             */
            interface ICommandEnvelope extends evohime.desktop.v1.CommandEnvelope.$Properties {
            }

            /** Represents a CommandEnvelope. */
            class CommandEnvelope {

                /**
                 * Constructs a new CommandEnvelope.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.CommandEnvelope.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** CommandEnvelope protocol. */
                protocol?: (evohime.desktop.v1.ProtocolVersion.$Properties|null);

                /** CommandEnvelope requestId. */
                requestId: string;

                /** CommandEnvelope clientId. */
                clientId: string;

                /** CommandEnvelope coreInstanceId. */
                coreInstanceId: string;

                /** CommandEnvelope sessionEpoch. */
                sessionEpoch: number;

                /** CommandEnvelope handshake. */
                handshake?: (evohime.desktop.v1.Handshake.$Properties|null);

                /** CommandEnvelope replayEvents. */
                replayEvents?: (evohime.desktop.v1.ReplayEvents.$Properties|null);

                /** CommandEnvelope startTask. */
                startTask?: (evohime.desktop.v1.StartTask.$Properties|null);

                /** CommandEnvelope stopTask. */
                stopTask?: (evohime.desktop.v1.StopTask.$Properties|null);

                /** CommandEnvelope resolveApproval. */
                resolveApproval?: (evohime.desktop.v1.ResolveApproval.$Properties|null);

                /** CommandEnvelope modelConfig. */
                modelConfig?: (evohime.desktop.v1.ModelConfigRequest.$Properties|null);

                /** CommandEnvelope modelCatalog. */
                modelCatalog?: (evohime.desktop.v1.ModelCatalogRequest.$Properties|null);

                /** CommandEnvelope permissionMode. */
                permissionMode?: (evohime.desktop.v1.PermissionModeRequest.$Properties|null);

                /** CommandEnvelope createProject. */
                createProject?: (evohime.desktop.v1.CreateProject.$Properties|null);

                /** CommandEnvelope createTask. */
                createTask?: (evohime.desktop.v1.CreateTask.$Properties|null);

                /** CommandEnvelope updateTaskStatus. */
                updateTaskStatus?: (evohime.desktop.v1.UpdateTaskStatus.$Properties|null);

                /** CommandEnvelope addTaskEdge. */
                addTaskEdge?: (evohime.desktop.v1.AddTaskEdge.$Properties|null);

                /** CommandEnvelope getTaskGraph. */
                getTaskGraph?: (evohime.desktop.v1.GetTaskGraph.$Properties|null);

                /** CommandEnvelope nextReadyTask. */
                nextReadyTask?: (evohime.desktop.v1.NextReadyTask.$Properties|null);

                /** CommandEnvelope importPrd. */
                importPrd?: (evohime.desktop.v1.ImportPrd.$Properties|null);

                /** CommandEnvelope getTaskHistory. */
                getTaskHistory?: (evohime.desktop.v1.GetTaskHistory.$Properties|null);

                /** CommandEnvelope getTaskContext. */
                getTaskContext?: (evohime.desktop.v1.GetTaskContext.$Properties|null);

                /** CommandEnvelope getTaskPlanSpec. */
                getTaskPlanSpec?: (evohime.desktop.v1.GetTaskPlanSpec.$Properties|null);

                /** CommandEnvelope applyApprovedBuild. */
                applyApprovedBuild?: (evohime.desktop.v1.ApplyApprovedBuild.$Properties|null);

                /** CommandEnvelope prepareBuild. */
                prepareBuild?: (evohime.desktop.v1.PrepareBuild.$Properties|null);

                /** CommandEnvelope getTaskSnapshot. */
                getTaskSnapshot?: (evohime.desktop.v1.GetTaskSnapshot.$Properties|null);

                /** CommandEnvelope restoreTaskSnapshot. */
                restoreTaskSnapshot?: (evohime.desktop.v1.RestoreTaskSnapshot.$Properties|null);

                /** CommandEnvelope getBuildPolicy. */
                getBuildPolicy?: (evohime.desktop.v1.GetBuildPolicy.$Properties|null);

                /** CommandEnvelope saveBuildPolicy. */
                saveBuildPolicy?: (evohime.desktop.v1.SaveBuildPolicy.$Properties|null);

                /** CommandEnvelope resyncRequest. */
                resyncRequest?: (evohime.desktop.v1.ResyncRequest.$Properties|null);

                /** CommandEnvelope runDoctor. */
                runDoctor?: (evohime.desktop.v1.RunDoctor.$Properties|null);

                /** CommandEnvelope saveResearchEvidence. */
                saveResearchEvidence?: (evohime.desktop.v1.SaveResearchEvidence.$Properties|null);

                /** CommandEnvelope listResearchEvidence. */
                listResearchEvidence?: (evohime.desktop.v1.ListResearchEvidence.$Properties|null);

                /** CommandEnvelope createMemory. */
                createMemory?: (evohime.desktop.v1.CreateMemory.$Properties|null);

                /** CommandEnvelope listMemory. */
                listMemory?: (evohime.desktop.v1.ListMemory.$Properties|null);

                /** CommandEnvelope searchMemory. */
                searchMemory?: (evohime.desktop.v1.SearchMemory.$Properties|null);

                /** CommandEnvelope archiveMemory. */
                archiveMemory?: (evohime.desktop.v1.ArchiveMemory.$Properties|null);

                /** CommandEnvelope forgetMemory. */
                forgetMemory?: (evohime.desktop.v1.ForgetMemory.$Properties|null);

                /** CommandEnvelope installCapability. */
                installCapability?: (evohime.desktop.v1.InstallCapability.$Properties|null);

                /** CommandEnvelope listCapabilities. */
                listCapabilities?: (evohime.desktop.v1.ListCapabilities.$Properties|null);

                /** CommandEnvelope matchCapabilities. */
                matchCapabilities?: (evohime.desktop.v1.MatchCapabilities.$Properties|null);

                /** CommandEnvelope removeCapability. */
                removeCapability?: (evohime.desktop.v1.RemoveCapability.$Properties|null);

                /** CommandEnvelope requestChildHandoff. */
                requestChildHandoff?: (evohime.desktop.v1.RequestChildHandoff.$Properties|null);

                /** CommandEnvelope listChildHandoffs. */
                listChildHandoffs?: (evohime.desktop.v1.ListChildHandoffs.$Properties|null);

                /** CommandEnvelope submitChildRequest. */
                submitChildRequest?: (evohime.desktop.v1.SubmitChildRequest.$Properties|null);

                /** CommandEnvelope submitChildReport. */
                submitChildReport?: (evohime.desktop.v1.SubmitChildReport.$Properties|null);

                /** CommandEnvelope runResearchFetch. */
                runResearchFetch?: (evohime.desktop.v1.RunResearchFetch.$Properties|null);

                /** CommandEnvelope listWorkspace. */
                listWorkspace?: (evohime.desktop.v1.ListWorkspace.$Properties|null);

                /** CommandEnvelope readWorkspaceFile. */
                readWorkspaceFile?: (evohime.desktop.v1.ReadWorkspaceFile.$Properties|null);

                /** CommandEnvelope gitStatus. */
                gitStatus?: (evohime.desktop.v1.GitStatus.$Properties|null);

                /** CommandEnvelope gitDiff. */
                gitDiff?: (evohime.desktop.v1.GitDiff.$Properties|null);

                /** CommandEnvelope terminalExecute. */
                terminalExecute?: (evohime.desktop.v1.TerminalExecute.$Properties|null);

                /** CommandEnvelope exportDoctorLogs. */
                exportDoctorLogs?: (evohime.desktop.v1.ExportDoctorLogs.$Properties|null);

                /** CommandEnvelope getCapabilitySelection. */
                getCapabilitySelection?: (evohime.desktop.v1.GetCapabilitySelection.$Properties|null);

                /** CommandEnvelope pinCapabilitySelection. */
                pinCapabilitySelection?: (evohime.desktop.v1.PinCapabilitySelection.$Properties|null);

                /** CommandEnvelope replaceCapabilitySelection. */
                replaceCapabilitySelection?: (evohime.desktop.v1.ReplaceCapabilitySelection.$Properties|null);

                /** CommandEnvelope submitFeedback. */
                submitFeedback?: (evohime.desktop.v1.SubmitFeedback.$Properties|null);

                /** CommandEnvelope listFeedback. */
                listFeedback?: (evohime.desktop.v1.ListFeedback.$Properties|null);

                /** CommandEnvelope createDatabaseBackup. */
                createDatabaseBackup?: (evohime.desktop.v1.CreateDatabaseBackup.$Properties|null);

                /** CommandEnvelope prepareDatabaseRestore. */
                prepareDatabaseRestore?: (evohime.desktop.v1.PrepareDatabaseRestore.$Properties|null);

                /** CommandEnvelope restoreDatabase. */
                restoreDatabase?: (evohime.desktop.v1.RestoreDatabase.$Properties|null);

                /** CommandEnvelope selectModel. */
                selectModel?: (evohime.desktop.v1.SelectModelRequest.$Properties|null);

                /** CommandEnvelope cancelDatabaseOperation. */
                cancelDatabaseOperation?: (evohime.desktop.v1.CancelDatabaseOperation.$Properties|null);

                /** CommandEnvelope getMemory. */
                getMemory?: (evohime.desktop.v1.GetMemory.$Properties|null);

                /** CommandEnvelope listMemoryPending. */
                listMemoryPending?: (evohime.desktop.v1.ListMemoryPending.$Properties|null);

                /** CommandEnvelope getMemoryConflicts. */
                getMemoryConflicts?: (evohime.desktop.v1.GetMemoryConflicts.$Properties|null);

                /** CommandEnvelope confirmMemory. */
                confirmMemory?: (evohime.desktop.v1.ConfirmMemory.$Properties|null);

                /** CommandEnvelope rejectMemory. */
                rejectMemory?: (evohime.desktop.v1.RejectMemory.$Properties|null);

                /** CommandEnvelope supersedeMemory. */
                supersedeMemory?: (evohime.desktop.v1.SupersedeMemory.$Properties|null);

                /** CommandEnvelope reviseMemoryCandidate. */
                reviseMemoryCandidate?: (evohime.desktop.v1.ReviseMemoryCandidate.$Properties|null);

                /** CommandEnvelope startPlanReview. */
                startPlanReview?: (evohime.desktop.v1.StartPlanReview.$Properties|null);

                /** CommandEnvelope stopPlanReview. */
                stopPlanReview?: (evohime.desktop.v1.StopPlanReview.$Properties|null);

                /** CommandEnvelope listPlanReviews. */
                listPlanReviews?: (evohime.desktop.v1.ListPlanReviews.$Properties|null);

                /** CommandEnvelope getPlanReview. */
                getPlanReview?: (evohime.desktop.v1.GetPlanReview.$Properties|null);

                /** CommandEnvelope exportPlanReview. */
                exportPlanReview?: (evohime.desktop.v1.ExportPlanReview.$Properties|null);

                /** CommandEnvelope clearPlanReviewHistory. */
                clearPlanReviewHistory?: (evohime.desktop.v1.ClearPlanReviewHistory.$Properties|null);

                /** CommandEnvelope getContextLedger. */
                getContextLedger?: (evohime.desktop.v1.GetContextLedger.$Properties|null);

                /** CommandEnvelope listTaskScratchpad. */
                listTaskScratchpad?: (evohime.desktop.v1.ListTaskScratchpad.$Properties|null);

                /** CommandEnvelope clearTaskScratchpad. */
                clearTaskScratchpad?: (evohime.desktop.v1.ClearTaskScratchpad.$Properties|null);

                /** CommandEnvelope summarizeContextNow. */
                summarizeContextNow?: (evohime.desktop.v1.SummarizeContextNow.$Properties|null);

                /** CommandEnvelope pinContextItem. */
                pinContextItem?: (evohime.desktop.v1.PinContextItem.$Properties|null);

                /** CommandEnvelope readContextArtifact. */
                readContextArtifact?: (evohime.desktop.v1.ReadContextArtifact.$Properties|null);

                /** CommandEnvelope indexWorkspace. */
                indexWorkspace?: (evohime.desktop.v1.IndexWorkspace.$Properties|null);

                /** CommandEnvelope rebuildIndex. */
                rebuildIndex?: (evohime.desktop.v1.RebuildIndex.$Properties|null);

                /** CommandEnvelope searchWorkspaceKnowledge. */
                searchWorkspaceKnowledge?: (evohime.desktop.v1.SearchWorkspaceKnowledge.$Properties|null);

                /** CommandEnvelope getIndexStatus. */
                getIndexStatus?: (evohime.desktop.v1.GetIndexStatus.$Properties|null);

                /** CommandEnvelope cancelWorkspaceIndex. */
                cancelWorkspaceIndex?: (evohime.desktop.v1.CancelWorkspaceIndex.$Properties|null);

                /** CommandEnvelope rotateReceiptKey. */
                rotateReceiptKey?: (evohime.desktop.v1.RotateReceiptKey.$Properties|null);

                /** CommandEnvelope trustReceiptGenesis. */
                trustReceiptGenesis?: (evohime.desktop.v1.TrustReceiptGenesis.$Properties|null);

                /** CommandEnvelope getReceiptKeyStatus. */
                getReceiptKeyStatus?: (evohime.desktop.v1.GetReceiptKeyStatus.$Properties|null);

                /** CommandEnvelope createNewReceiptGenesis. */
                createNewReceiptGenesis?: (evohime.desktop.v1.CreateNewReceiptGenesis.$Properties|null);

                /** CommandEnvelope closePendingReceiptAction. */
                closePendingReceiptAction?: (evohime.desktop.v1.ClosePendingReceiptAction.$Properties|null);

                /** CommandEnvelope setReceiptAuditSamplingRate. */
                setReceiptAuditSamplingRate?: (evohime.desktop.v1.SetReceiptAuditSamplingRate.$Properties|null);

                /** CommandEnvelope reconcilePendingReceiptAction. */
                reconcilePendingReceiptAction?: (evohime.desktop.v1.ReconcilePendingReceiptAction.$Properties|null);

                /** CommandEnvelope unquarantineReceiptAction. */
                unquarantineReceiptAction?: (evohime.desktop.v1.UnquarantineReceiptAction.$Properties|null);

                /** CommandEnvelope listReceipts. */
                listReceipts?: (evohime.desktop.v1.ListReceipts.$Properties|null);

                /** CommandEnvelope verifyReceipts. */
                verifyReceipts?: (evohime.desktop.v1.VerifyReceipts.$Properties|null);

                /** CommandEnvelope exportReceipts. */
                exportReceipts?: (evohime.desktop.v1.ExportReceipts.$Properties|null);

                /** CommandEnvelope revisePlan. */
                revisePlan?: (evohime.desktop.v1.RevisePlan.$Properties|null);

                /** CommandEnvelope stopRevision. */
                stopRevision?: (evohime.desktop.v1.StopRevision.$Properties|null);

                /** CommandEnvelope saveRevisedPlan. */
                saveRevisedPlan?: (evohime.desktop.v1.SaveRevisedPlan.$Properties|null);

                /** CommandEnvelope resolveRoutingDecision. */
                resolveRoutingDecision?: (evohime.desktop.v1.ResolveRoutingDecision.$Properties|null);

                /** CommandEnvelope setAmbientListening. */
                setAmbientListening?: (evohime.desktop.v1.SetAmbientListening.$Properties|null);

                /** CommandEnvelope getAmbientStatus. */
                getAmbientStatus?: (evohime.desktop.v1.GetAmbientStatus.$Properties|null);

                /** CommandEnvelope listAmbientEpisodes. */
                listAmbientEpisodes?: (evohime.desktop.v1.ListAmbientEpisodes.$Properties|null);

                /** CommandEnvelope getAmbientEpisode. */
                getAmbientEpisode?: (evohime.desktop.v1.GetAmbientEpisode.$Properties|null);

                /** CommandEnvelope deleteAmbientTranscripts. */
                deleteAmbientTranscripts?: (evohime.desktop.v1.DeleteAmbientTranscripts.$Properties|null);

                /** CommandEnvelope forgetAmbientWindow. */
                forgetAmbientWindow?: (evohime.desktop.v1.ForgetAmbientWindow.$Properties|null);

                /** CommandEnvelope getAmbientPolicy. */
                getAmbientPolicy?: (evohime.desktop.v1.GetAmbientPolicy.$Properties|null);

                /** CommandEnvelope saveAmbientPolicy. */
                saveAmbientPolicy?: (evohime.desktop.v1.SaveAmbientPolicy.$Properties|null);

                /** CommandEnvelope resolveAmbientProposal. */
                resolveAmbientProposal?: (evohime.desktop.v1.ResolveAmbientProposal.$Properties|null);

                /** CommandEnvelope listAmbientProposals. */
                listAmbientProposals?: (evohime.desktop.v1.ListAmbientProposals.$Properties|null);

                /** CommandEnvelope listWorkflowTemplates. */
                listWorkflowTemplates?: (evohime.desktop.v1.ListWorkflowTemplates.$Properties|null);

                /** CommandEnvelope getWorkflowDefinition. */
                getWorkflowDefinition?: (evohime.desktop.v1.GetWorkflowDefinition.$Properties|null);

                /** CommandEnvelope startWorkflow. */
                startWorkflow?: (evohime.desktop.v1.StartWorkflow.$Properties|null);

                /** CommandEnvelope getWorkflowRun. */
                getWorkflowRun?: (evohime.desktop.v1.GetWorkflowRun.$Properties|null);

                /** CommandEnvelope cancelWorkflow. */
                cancelWorkflow?: (evohime.desktop.v1.CancelWorkflow.$Properties|null);

                /** CommandEnvelope listWorkflowEvents. */
                listWorkflowEvents?: (evohime.desktop.v1.ListWorkflowEvents.$Properties|null);

                /** CommandEnvelope command. */
                command?: ("handshake"|"replayEvents"|"startTask"|"stopTask"|"resolveApproval"|"modelConfig"|"modelCatalog"|"permissionMode"|"createProject"|"createTask"|"updateTaskStatus"|"addTaskEdge"|"getTaskGraph"|"nextReadyTask"|"importPrd"|"getTaskHistory"|"getTaskContext"|"getTaskPlanSpec"|"applyApprovedBuild"|"prepareBuild"|"getTaskSnapshot"|"restoreTaskSnapshot"|"getBuildPolicy"|"saveBuildPolicy"|"resyncRequest"|"runDoctor"|"saveResearchEvidence"|"listResearchEvidence"|"createMemory"|"listMemory"|"searchMemory"|"archiveMemory"|"forgetMemory"|"installCapability"|"listCapabilities"|"matchCapabilities"|"removeCapability"|"requestChildHandoff"|"listChildHandoffs"|"submitChildRequest"|"submitChildReport"|"runResearchFetch"|"listWorkspace"|"readWorkspaceFile"|"gitStatus"|"gitDiff"|"terminalExecute"|"exportDoctorLogs"|"getCapabilitySelection"|"pinCapabilitySelection"|"replaceCapabilitySelection"|"submitFeedback"|"listFeedback"|"createDatabaseBackup"|"prepareDatabaseRestore"|"restoreDatabase"|"selectModel"|"cancelDatabaseOperation"|"getMemory"|"listMemoryPending"|"getMemoryConflicts"|"confirmMemory"|"rejectMemory"|"supersedeMemory"|"reviseMemoryCandidate"|"startPlanReview"|"stopPlanReview"|"listPlanReviews"|"getPlanReview"|"exportPlanReview"|"clearPlanReviewHistory"|"getContextLedger"|"listTaskScratchpad"|"clearTaskScratchpad"|"summarizeContextNow"|"pinContextItem"|"readContextArtifact"|"indexWorkspace"|"rebuildIndex"|"searchWorkspaceKnowledge"|"getIndexStatus"|"cancelWorkspaceIndex"|"rotateReceiptKey"|"trustReceiptGenesis"|"getReceiptKeyStatus"|"createNewReceiptGenesis"|"closePendingReceiptAction"|"setReceiptAuditSamplingRate"|"reconcilePendingReceiptAction"|"unquarantineReceiptAction"|"listReceipts"|"verifyReceipts"|"exportReceipts"|"revisePlan"|"stopRevision"|"saveRevisedPlan"|"resolveRoutingDecision"|"setAmbientListening"|"getAmbientStatus"|"listAmbientEpisodes"|"getAmbientEpisode"|"deleteAmbientTranscripts"|"forgetAmbientWindow"|"getAmbientPolicy"|"saveAmbientPolicy"|"resolveAmbientProposal"|"listAmbientProposals"|"listWorkflowTemplates"|"getWorkflowDefinition"|"startWorkflow"|"getWorkflowRun"|"cancelWorkflow"|"listWorkflowEvents");

                /**
                 * Encodes the specified CommandEnvelope message. Does not implicitly {@link evohime.desktop.v1.CommandEnvelope.verify|verify} messages.
                 * @param message CommandEnvelope message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.CommandEnvelope.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CommandEnvelope message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.CommandEnvelope & evohime.desktop.v1.CommandEnvelope.$Shape} CommandEnvelope
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.CommandEnvelope & evohime.desktop.v1.CommandEnvelope.$Shape;

                /**
                 * Gets the type url for CommandEnvelope
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace CommandEnvelope {

                /** Properties of a CommandEnvelope. */
                interface $Properties {

                    /** CommandEnvelope protocol */
                    protocol?: (evohime.desktop.v1.ProtocolVersion.$Properties|null);

                    /** CommandEnvelope requestId */
                    requestId?: (string|null);

                    /** CommandEnvelope clientId */
                    clientId?: (string|null);

                    /** CommandEnvelope coreInstanceId */
                    coreInstanceId?: (string|null);

                    /** CommandEnvelope sessionEpoch */
                    sessionEpoch?: (number|null);

                    /** CommandEnvelope handshake */
                    handshake?: (evohime.desktop.v1.Handshake.$Properties|null);

                    /** CommandEnvelope replayEvents */
                    replayEvents?: (evohime.desktop.v1.ReplayEvents.$Properties|null);

                    /** CommandEnvelope startTask */
                    startTask?: (evohime.desktop.v1.StartTask.$Properties|null);

                    /** CommandEnvelope stopTask */
                    stopTask?: (evohime.desktop.v1.StopTask.$Properties|null);

                    /** CommandEnvelope resolveApproval */
                    resolveApproval?: (evohime.desktop.v1.ResolveApproval.$Properties|null);

                    /** CommandEnvelope modelConfig */
                    modelConfig?: (evohime.desktop.v1.ModelConfigRequest.$Properties|null);

                    /** CommandEnvelope modelCatalog */
                    modelCatalog?: (evohime.desktop.v1.ModelCatalogRequest.$Properties|null);

                    /** CommandEnvelope permissionMode */
                    permissionMode?: (evohime.desktop.v1.PermissionModeRequest.$Properties|null);

                    /** CommandEnvelope createProject */
                    createProject?: (evohime.desktop.v1.CreateProject.$Properties|null);

                    /** CommandEnvelope createTask */
                    createTask?: (evohime.desktop.v1.CreateTask.$Properties|null);

                    /** CommandEnvelope updateTaskStatus */
                    updateTaskStatus?: (evohime.desktop.v1.UpdateTaskStatus.$Properties|null);

                    /** CommandEnvelope addTaskEdge */
                    addTaskEdge?: (evohime.desktop.v1.AddTaskEdge.$Properties|null);

                    /** CommandEnvelope getTaskGraph */
                    getTaskGraph?: (evohime.desktop.v1.GetTaskGraph.$Properties|null);

                    /** CommandEnvelope nextReadyTask */
                    nextReadyTask?: (evohime.desktop.v1.NextReadyTask.$Properties|null);

                    /** CommandEnvelope importPrd */
                    importPrd?: (evohime.desktop.v1.ImportPrd.$Properties|null);

                    /** CommandEnvelope getTaskHistory */
                    getTaskHistory?: (evohime.desktop.v1.GetTaskHistory.$Properties|null);

                    /** CommandEnvelope getTaskContext */
                    getTaskContext?: (evohime.desktop.v1.GetTaskContext.$Properties|null);

                    /** CommandEnvelope getTaskPlanSpec */
                    getTaskPlanSpec?: (evohime.desktop.v1.GetTaskPlanSpec.$Properties|null);

                    /** CommandEnvelope applyApprovedBuild */
                    applyApprovedBuild?: (evohime.desktop.v1.ApplyApprovedBuild.$Properties|null);

                    /** CommandEnvelope prepareBuild */
                    prepareBuild?: (evohime.desktop.v1.PrepareBuild.$Properties|null);

                    /** CommandEnvelope getTaskSnapshot */
                    getTaskSnapshot?: (evohime.desktop.v1.GetTaskSnapshot.$Properties|null);

                    /** CommandEnvelope restoreTaskSnapshot */
                    restoreTaskSnapshot?: (evohime.desktop.v1.RestoreTaskSnapshot.$Properties|null);

                    /** CommandEnvelope getBuildPolicy */
                    getBuildPolicy?: (evohime.desktop.v1.GetBuildPolicy.$Properties|null);

                    /** CommandEnvelope saveBuildPolicy */
                    saveBuildPolicy?: (evohime.desktop.v1.SaveBuildPolicy.$Properties|null);

                    /** CommandEnvelope resyncRequest */
                    resyncRequest?: (evohime.desktop.v1.ResyncRequest.$Properties|null);

                    /** CommandEnvelope runDoctor */
                    runDoctor?: (evohime.desktop.v1.RunDoctor.$Properties|null);

                    /** CommandEnvelope saveResearchEvidence */
                    saveResearchEvidence?: (evohime.desktop.v1.SaveResearchEvidence.$Properties|null);

                    /** CommandEnvelope listResearchEvidence */
                    listResearchEvidence?: (evohime.desktop.v1.ListResearchEvidence.$Properties|null);

                    /** CommandEnvelope createMemory */
                    createMemory?: (evohime.desktop.v1.CreateMemory.$Properties|null);

                    /** CommandEnvelope listMemory */
                    listMemory?: (evohime.desktop.v1.ListMemory.$Properties|null);

                    /** CommandEnvelope searchMemory */
                    searchMemory?: (evohime.desktop.v1.SearchMemory.$Properties|null);

                    /** CommandEnvelope archiveMemory */
                    archiveMemory?: (evohime.desktop.v1.ArchiveMemory.$Properties|null);

                    /** CommandEnvelope forgetMemory */
                    forgetMemory?: (evohime.desktop.v1.ForgetMemory.$Properties|null);

                    /** CommandEnvelope installCapability */
                    installCapability?: (evohime.desktop.v1.InstallCapability.$Properties|null);

                    /** CommandEnvelope listCapabilities */
                    listCapabilities?: (evohime.desktop.v1.ListCapabilities.$Properties|null);

                    /** CommandEnvelope matchCapabilities */
                    matchCapabilities?: (evohime.desktop.v1.MatchCapabilities.$Properties|null);

                    /** CommandEnvelope removeCapability */
                    removeCapability?: (evohime.desktop.v1.RemoveCapability.$Properties|null);

                    /** CommandEnvelope requestChildHandoff */
                    requestChildHandoff?: (evohime.desktop.v1.RequestChildHandoff.$Properties|null);

                    /** CommandEnvelope listChildHandoffs */
                    listChildHandoffs?: (evohime.desktop.v1.ListChildHandoffs.$Properties|null);

                    /** CommandEnvelope submitChildRequest */
                    submitChildRequest?: (evohime.desktop.v1.SubmitChildRequest.$Properties|null);

                    /** CommandEnvelope submitChildReport */
                    submitChildReport?: (evohime.desktop.v1.SubmitChildReport.$Properties|null);

                    /** CommandEnvelope runResearchFetch */
                    runResearchFetch?: (evohime.desktop.v1.RunResearchFetch.$Properties|null);

                    /** CommandEnvelope listWorkspace */
                    listWorkspace?: (evohime.desktop.v1.ListWorkspace.$Properties|null);

                    /** CommandEnvelope readWorkspaceFile */
                    readWorkspaceFile?: (evohime.desktop.v1.ReadWorkspaceFile.$Properties|null);

                    /** CommandEnvelope gitStatus */
                    gitStatus?: (evohime.desktop.v1.GitStatus.$Properties|null);

                    /** CommandEnvelope gitDiff */
                    gitDiff?: (evohime.desktop.v1.GitDiff.$Properties|null);

                    /** CommandEnvelope terminalExecute */
                    terminalExecute?: (evohime.desktop.v1.TerminalExecute.$Properties|null);

                    /** CommandEnvelope exportDoctorLogs */
                    exportDoctorLogs?: (evohime.desktop.v1.ExportDoctorLogs.$Properties|null);

                    /** CommandEnvelope getCapabilitySelection */
                    getCapabilitySelection?: (evohime.desktop.v1.GetCapabilitySelection.$Properties|null);

                    /** CommandEnvelope pinCapabilitySelection */
                    pinCapabilitySelection?: (evohime.desktop.v1.PinCapabilitySelection.$Properties|null);

                    /** CommandEnvelope replaceCapabilitySelection */
                    replaceCapabilitySelection?: (evohime.desktop.v1.ReplaceCapabilitySelection.$Properties|null);

                    /** CommandEnvelope submitFeedback */
                    submitFeedback?: (evohime.desktop.v1.SubmitFeedback.$Properties|null);

                    /** CommandEnvelope listFeedback */
                    listFeedback?: (evohime.desktop.v1.ListFeedback.$Properties|null);

                    /** CommandEnvelope createDatabaseBackup */
                    createDatabaseBackup?: (evohime.desktop.v1.CreateDatabaseBackup.$Properties|null);

                    /** CommandEnvelope prepareDatabaseRestore */
                    prepareDatabaseRestore?: (evohime.desktop.v1.PrepareDatabaseRestore.$Properties|null);

                    /** CommandEnvelope restoreDatabase */
                    restoreDatabase?: (evohime.desktop.v1.RestoreDatabase.$Properties|null);

                    /** CommandEnvelope selectModel */
                    selectModel?: (evohime.desktop.v1.SelectModelRequest.$Properties|null);

                    /** CommandEnvelope cancelDatabaseOperation */
                    cancelDatabaseOperation?: (evohime.desktop.v1.CancelDatabaseOperation.$Properties|null);

                    /** CommandEnvelope getMemory */
                    getMemory?: (evohime.desktop.v1.GetMemory.$Properties|null);

                    /** CommandEnvelope listMemoryPending */
                    listMemoryPending?: (evohime.desktop.v1.ListMemoryPending.$Properties|null);

                    /** CommandEnvelope getMemoryConflicts */
                    getMemoryConflicts?: (evohime.desktop.v1.GetMemoryConflicts.$Properties|null);

                    /** CommandEnvelope confirmMemory */
                    confirmMemory?: (evohime.desktop.v1.ConfirmMemory.$Properties|null);

                    /** CommandEnvelope rejectMemory */
                    rejectMemory?: (evohime.desktop.v1.RejectMemory.$Properties|null);

                    /** CommandEnvelope supersedeMemory */
                    supersedeMemory?: (evohime.desktop.v1.SupersedeMemory.$Properties|null);

                    /** CommandEnvelope reviseMemoryCandidate */
                    reviseMemoryCandidate?: (evohime.desktop.v1.ReviseMemoryCandidate.$Properties|null);

                    /** CommandEnvelope startPlanReview */
                    startPlanReview?: (evohime.desktop.v1.StartPlanReview.$Properties|null);

                    /** CommandEnvelope stopPlanReview */
                    stopPlanReview?: (evohime.desktop.v1.StopPlanReview.$Properties|null);

                    /** CommandEnvelope listPlanReviews */
                    listPlanReviews?: (evohime.desktop.v1.ListPlanReviews.$Properties|null);

                    /** CommandEnvelope getPlanReview */
                    getPlanReview?: (evohime.desktop.v1.GetPlanReview.$Properties|null);

                    /** CommandEnvelope exportPlanReview */
                    exportPlanReview?: (evohime.desktop.v1.ExportPlanReview.$Properties|null);

                    /** CommandEnvelope clearPlanReviewHistory */
                    clearPlanReviewHistory?: (evohime.desktop.v1.ClearPlanReviewHistory.$Properties|null);

                    /** CommandEnvelope getContextLedger */
                    getContextLedger?: (evohime.desktop.v1.GetContextLedger.$Properties|null);

                    /** CommandEnvelope listTaskScratchpad */
                    listTaskScratchpad?: (evohime.desktop.v1.ListTaskScratchpad.$Properties|null);

                    /** CommandEnvelope clearTaskScratchpad */
                    clearTaskScratchpad?: (evohime.desktop.v1.ClearTaskScratchpad.$Properties|null);

                    /** CommandEnvelope summarizeContextNow */
                    summarizeContextNow?: (evohime.desktop.v1.SummarizeContextNow.$Properties|null);

                    /** CommandEnvelope pinContextItem */
                    pinContextItem?: (evohime.desktop.v1.PinContextItem.$Properties|null);

                    /** CommandEnvelope readContextArtifact */
                    readContextArtifact?: (evohime.desktop.v1.ReadContextArtifact.$Properties|null);

                    /** CommandEnvelope indexWorkspace */
                    indexWorkspace?: (evohime.desktop.v1.IndexWorkspace.$Properties|null);

                    /** CommandEnvelope rebuildIndex */
                    rebuildIndex?: (evohime.desktop.v1.RebuildIndex.$Properties|null);

                    /** CommandEnvelope searchWorkspaceKnowledge */
                    searchWorkspaceKnowledge?: (evohime.desktop.v1.SearchWorkspaceKnowledge.$Properties|null);

                    /** CommandEnvelope getIndexStatus */
                    getIndexStatus?: (evohime.desktop.v1.GetIndexStatus.$Properties|null);

                    /** CommandEnvelope cancelWorkspaceIndex */
                    cancelWorkspaceIndex?: (evohime.desktop.v1.CancelWorkspaceIndex.$Properties|null);

                    /** CommandEnvelope rotateReceiptKey */
                    rotateReceiptKey?: (evohime.desktop.v1.RotateReceiptKey.$Properties|null);

                    /** CommandEnvelope trustReceiptGenesis */
                    trustReceiptGenesis?: (evohime.desktop.v1.TrustReceiptGenesis.$Properties|null);

                    /** CommandEnvelope getReceiptKeyStatus */
                    getReceiptKeyStatus?: (evohime.desktop.v1.GetReceiptKeyStatus.$Properties|null);

                    /** CommandEnvelope createNewReceiptGenesis */
                    createNewReceiptGenesis?: (evohime.desktop.v1.CreateNewReceiptGenesis.$Properties|null);

                    /** CommandEnvelope closePendingReceiptAction */
                    closePendingReceiptAction?: (evohime.desktop.v1.ClosePendingReceiptAction.$Properties|null);

                    /** CommandEnvelope setReceiptAuditSamplingRate */
                    setReceiptAuditSamplingRate?: (evohime.desktop.v1.SetReceiptAuditSamplingRate.$Properties|null);

                    /** CommandEnvelope reconcilePendingReceiptAction */
                    reconcilePendingReceiptAction?: (evohime.desktop.v1.ReconcilePendingReceiptAction.$Properties|null);

                    /** CommandEnvelope unquarantineReceiptAction */
                    unquarantineReceiptAction?: (evohime.desktop.v1.UnquarantineReceiptAction.$Properties|null);

                    /** CommandEnvelope listReceipts */
                    listReceipts?: (evohime.desktop.v1.ListReceipts.$Properties|null);

                    /** CommandEnvelope verifyReceipts */
                    verifyReceipts?: (evohime.desktop.v1.VerifyReceipts.$Properties|null);

                    /** CommandEnvelope exportReceipts */
                    exportReceipts?: (evohime.desktop.v1.ExportReceipts.$Properties|null);

                    /** CommandEnvelope revisePlan */
                    revisePlan?: (evohime.desktop.v1.RevisePlan.$Properties|null);

                    /** CommandEnvelope stopRevision */
                    stopRevision?: (evohime.desktop.v1.StopRevision.$Properties|null);

                    /** CommandEnvelope saveRevisedPlan */
                    saveRevisedPlan?: (evohime.desktop.v1.SaveRevisedPlan.$Properties|null);

                    /** CommandEnvelope resolveRoutingDecision */
                    resolveRoutingDecision?: (evohime.desktop.v1.ResolveRoutingDecision.$Properties|null);

                    /** CommandEnvelope setAmbientListening */
                    setAmbientListening?: (evohime.desktop.v1.SetAmbientListening.$Properties|null);

                    /** CommandEnvelope getAmbientStatus */
                    getAmbientStatus?: (evohime.desktop.v1.GetAmbientStatus.$Properties|null);

                    /** CommandEnvelope listAmbientEpisodes */
                    listAmbientEpisodes?: (evohime.desktop.v1.ListAmbientEpisodes.$Properties|null);

                    /** CommandEnvelope getAmbientEpisode */
                    getAmbientEpisode?: (evohime.desktop.v1.GetAmbientEpisode.$Properties|null);

                    /** CommandEnvelope deleteAmbientTranscripts */
                    deleteAmbientTranscripts?: (evohime.desktop.v1.DeleteAmbientTranscripts.$Properties|null);

                    /** CommandEnvelope forgetAmbientWindow */
                    forgetAmbientWindow?: (evohime.desktop.v1.ForgetAmbientWindow.$Properties|null);

                    /** CommandEnvelope getAmbientPolicy */
                    getAmbientPolicy?: (evohime.desktop.v1.GetAmbientPolicy.$Properties|null);

                    /** CommandEnvelope saveAmbientPolicy */
                    saveAmbientPolicy?: (evohime.desktop.v1.SaveAmbientPolicy.$Properties|null);

                    /** CommandEnvelope resolveAmbientProposal */
                    resolveAmbientProposal?: (evohime.desktop.v1.ResolveAmbientProposal.$Properties|null);

                    /** CommandEnvelope listAmbientProposals */
                    listAmbientProposals?: (evohime.desktop.v1.ListAmbientProposals.$Properties|null);

                    /** CommandEnvelope listWorkflowTemplates */
                    listWorkflowTemplates?: (evohime.desktop.v1.ListWorkflowTemplates.$Properties|null);

                    /** CommandEnvelope getWorkflowDefinition */
                    getWorkflowDefinition?: (evohime.desktop.v1.GetWorkflowDefinition.$Properties|null);

                    /** CommandEnvelope startWorkflow */
                    startWorkflow?: (evohime.desktop.v1.StartWorkflow.$Properties|null);

                    /** CommandEnvelope getWorkflowRun */
                    getWorkflowRun?: (evohime.desktop.v1.GetWorkflowRun.$Properties|null);

                    /** CommandEnvelope cancelWorkflow */
                    cancelWorkflow?: (evohime.desktop.v1.CancelWorkflow.$Properties|null);

                    /** CommandEnvelope listWorkflowEvents */
                    listWorkflowEvents?: (evohime.desktop.v1.ListWorkflowEvents.$Properties|null);

                    /** CommandEnvelope command */
                    command?: ("handshake"|"replayEvents"|"startTask"|"stopTask"|"resolveApproval"|"modelConfig"|"modelCatalog"|"permissionMode"|"createProject"|"createTask"|"updateTaskStatus"|"addTaskEdge"|"getTaskGraph"|"nextReadyTask"|"importPrd"|"getTaskHistory"|"getTaskContext"|"getTaskPlanSpec"|"applyApprovedBuild"|"prepareBuild"|"getTaskSnapshot"|"restoreTaskSnapshot"|"getBuildPolicy"|"saveBuildPolicy"|"resyncRequest"|"runDoctor"|"saveResearchEvidence"|"listResearchEvidence"|"createMemory"|"listMemory"|"searchMemory"|"archiveMemory"|"forgetMemory"|"installCapability"|"listCapabilities"|"matchCapabilities"|"removeCapability"|"requestChildHandoff"|"listChildHandoffs"|"submitChildRequest"|"submitChildReport"|"runResearchFetch"|"listWorkspace"|"readWorkspaceFile"|"gitStatus"|"gitDiff"|"terminalExecute"|"exportDoctorLogs"|"getCapabilitySelection"|"pinCapabilitySelection"|"replaceCapabilitySelection"|"submitFeedback"|"listFeedback"|"createDatabaseBackup"|"prepareDatabaseRestore"|"restoreDatabase"|"selectModel"|"cancelDatabaseOperation"|"getMemory"|"listMemoryPending"|"getMemoryConflicts"|"confirmMemory"|"rejectMemory"|"supersedeMemory"|"reviseMemoryCandidate"|"startPlanReview"|"stopPlanReview"|"listPlanReviews"|"getPlanReview"|"exportPlanReview"|"clearPlanReviewHistory"|"getContextLedger"|"listTaskScratchpad"|"clearTaskScratchpad"|"summarizeContextNow"|"pinContextItem"|"readContextArtifact"|"indexWorkspace"|"rebuildIndex"|"searchWorkspaceKnowledge"|"getIndexStatus"|"cancelWorkspaceIndex"|"rotateReceiptKey"|"trustReceiptGenesis"|"getReceiptKeyStatus"|"createNewReceiptGenesis"|"closePendingReceiptAction"|"setReceiptAuditSamplingRate"|"reconcilePendingReceiptAction"|"unquarantineReceiptAction"|"listReceipts"|"verifyReceipts"|"exportReceipts"|"revisePlan"|"stopRevision"|"saveRevisedPlan"|"resolveRoutingDecision"|"setAmbientListening"|"getAmbientStatus"|"listAmbientEpisodes"|"getAmbientEpisode"|"deleteAmbientTranscripts"|"forgetAmbientWindow"|"getAmbientPolicy"|"saveAmbientPolicy"|"resolveAmbientProposal"|"listAmbientProposals"|"listWorkflowTemplates"|"getWorkflowDefinition"|"startWorkflow"|"getWorkflowRun"|"cancelWorkflow"|"listWorkflowEvents");

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Narrowed shape of a CommandEnvelope. */
                type $Shape = {
                  protocol?: evohime.desktop.v1.ProtocolVersion.$Shape|null;
                  requestId?: string|null;
                  clientId?: string|null;
                  coreInstanceId?: string|null;
                  sessionEpoch?: number|null;
                  handshake?: evohime.desktop.v1.Handshake.$Shape|null;
                  replayEvents?: evohime.desktop.v1.ReplayEvents.$Shape|null;
                  startTask?: evohime.desktop.v1.StartTask.$Shape|null;
                  stopTask?: evohime.desktop.v1.StopTask.$Shape|null;
                  resolveApproval?: evohime.desktop.v1.ResolveApproval.$Shape|null;
                  modelConfig?: evohime.desktop.v1.ModelConfigRequest.$Shape|null;
                  modelCatalog?: evohime.desktop.v1.ModelCatalogRequest.$Shape|null;
                  permissionMode?: evohime.desktop.v1.PermissionModeRequest.$Shape|null;
                  createProject?: evohime.desktop.v1.CreateProject.$Shape|null;
                  createTask?: evohime.desktop.v1.CreateTask.$Shape|null;
                  updateTaskStatus?: evohime.desktop.v1.UpdateTaskStatus.$Shape|null;
                  addTaskEdge?: evohime.desktop.v1.AddTaskEdge.$Shape|null;
                  getTaskGraph?: evohime.desktop.v1.GetTaskGraph.$Shape|null;
                  nextReadyTask?: evohime.desktop.v1.NextReadyTask.$Shape|null;
                  importPrd?: evohime.desktop.v1.ImportPrd.$Shape|null;
                  getTaskHistory?: evohime.desktop.v1.GetTaskHistory.$Shape|null;
                  getTaskContext?: evohime.desktop.v1.GetTaskContext.$Shape|null;
                  getTaskPlanSpec?: evohime.desktop.v1.GetTaskPlanSpec.$Shape|null;
                  applyApprovedBuild?: evohime.desktop.v1.ApplyApprovedBuild.$Shape|null;
                  prepareBuild?: evohime.desktop.v1.PrepareBuild.$Shape|null;
                  getTaskSnapshot?: evohime.desktop.v1.GetTaskSnapshot.$Shape|null;
                  restoreTaskSnapshot?: evohime.desktop.v1.RestoreTaskSnapshot.$Shape|null;
                  getBuildPolicy?: evohime.desktop.v1.GetBuildPolicy.$Shape|null;
                  saveBuildPolicy?: evohime.desktop.v1.SaveBuildPolicy.$Shape|null;
                  resyncRequest?: evohime.desktop.v1.ResyncRequest.$Shape|null;
                  runDoctor?: evohime.desktop.v1.RunDoctor.$Shape|null;
                  saveResearchEvidence?: evohime.desktop.v1.SaveResearchEvidence.$Shape|null;
                  listResearchEvidence?: evohime.desktop.v1.ListResearchEvidence.$Shape|null;
                  createMemory?: evohime.desktop.v1.CreateMemory.$Shape|null;
                  listMemory?: evohime.desktop.v1.ListMemory.$Shape|null;
                  searchMemory?: evohime.desktop.v1.SearchMemory.$Shape|null;
                  archiveMemory?: evohime.desktop.v1.ArchiveMemory.$Shape|null;
                  forgetMemory?: evohime.desktop.v1.ForgetMemory.$Shape|null;
                  installCapability?: evohime.desktop.v1.InstallCapability.$Shape|null;
                  listCapabilities?: evohime.desktop.v1.ListCapabilities.$Shape|null;
                  matchCapabilities?: evohime.desktop.v1.MatchCapabilities.$Shape|null;
                  removeCapability?: evohime.desktop.v1.RemoveCapability.$Shape|null;
                  requestChildHandoff?: evohime.desktop.v1.RequestChildHandoff.$Shape|null;
                  listChildHandoffs?: evohime.desktop.v1.ListChildHandoffs.$Shape|null;
                  submitChildRequest?: evohime.desktop.v1.SubmitChildRequest.$Shape|null;
                  submitChildReport?: evohime.desktop.v1.SubmitChildReport.$Shape|null;
                  runResearchFetch?: evohime.desktop.v1.RunResearchFetch.$Shape|null;
                  listWorkspace?: evohime.desktop.v1.ListWorkspace.$Shape|null;
                  readWorkspaceFile?: evohime.desktop.v1.ReadWorkspaceFile.$Shape|null;
                  gitStatus?: evohime.desktop.v1.GitStatus.$Shape|null;
                  gitDiff?: evohime.desktop.v1.GitDiff.$Shape|null;
                  terminalExecute?: evohime.desktop.v1.TerminalExecute.$Shape|null;
                  exportDoctorLogs?: evohime.desktop.v1.ExportDoctorLogs.$Shape|null;
                  getCapabilitySelection?: evohime.desktop.v1.GetCapabilitySelection.$Shape|null;
                  pinCapabilitySelection?: evohime.desktop.v1.PinCapabilitySelection.$Shape|null;
                  replaceCapabilitySelection?: evohime.desktop.v1.ReplaceCapabilitySelection.$Shape|null;
                  submitFeedback?: evohime.desktop.v1.SubmitFeedback.$Shape|null;
                  listFeedback?: evohime.desktop.v1.ListFeedback.$Shape|null;
                  createDatabaseBackup?: evohime.desktop.v1.CreateDatabaseBackup.$Shape|null;
                  prepareDatabaseRestore?: evohime.desktop.v1.PrepareDatabaseRestore.$Shape|null;
                  restoreDatabase?: evohime.desktop.v1.RestoreDatabase.$Shape|null;
                  selectModel?: evohime.desktop.v1.SelectModelRequest.$Shape|null;
                  cancelDatabaseOperation?: evohime.desktop.v1.CancelDatabaseOperation.$Shape|null;
                  getMemory?: evohime.desktop.v1.GetMemory.$Shape|null;
                  listMemoryPending?: evohime.desktop.v1.ListMemoryPending.$Shape|null;
                  getMemoryConflicts?: evohime.desktop.v1.GetMemoryConflicts.$Shape|null;
                  confirmMemory?: evohime.desktop.v1.ConfirmMemory.$Shape|null;
                  rejectMemory?: evohime.desktop.v1.RejectMemory.$Shape|null;
                  supersedeMemory?: evohime.desktop.v1.SupersedeMemory.$Shape|null;
                  reviseMemoryCandidate?: evohime.desktop.v1.ReviseMemoryCandidate.$Shape|null;
                  startPlanReview?: evohime.desktop.v1.StartPlanReview.$Shape|null;
                  stopPlanReview?: evohime.desktop.v1.StopPlanReview.$Shape|null;
                  listPlanReviews?: evohime.desktop.v1.ListPlanReviews.$Shape|null;
                  getPlanReview?: evohime.desktop.v1.GetPlanReview.$Shape|null;
                  exportPlanReview?: evohime.desktop.v1.ExportPlanReview.$Shape|null;
                  clearPlanReviewHistory?: evohime.desktop.v1.ClearPlanReviewHistory.$Shape|null;
                  getContextLedger?: evohime.desktop.v1.GetContextLedger.$Shape|null;
                  listTaskScratchpad?: evohime.desktop.v1.ListTaskScratchpad.$Shape|null;
                  clearTaskScratchpad?: evohime.desktop.v1.ClearTaskScratchpad.$Shape|null;
                  summarizeContextNow?: evohime.desktop.v1.SummarizeContextNow.$Shape|null;
                  pinContextItem?: evohime.desktop.v1.PinContextItem.$Shape|null;
                  readContextArtifact?: evohime.desktop.v1.ReadContextArtifact.$Shape|null;
                  indexWorkspace?: evohime.desktop.v1.IndexWorkspace.$Shape|null;
                  rebuildIndex?: evohime.desktop.v1.RebuildIndex.$Shape|null;
                  searchWorkspaceKnowledge?: evohime.desktop.v1.SearchWorkspaceKnowledge.$Shape|null;
                  getIndexStatus?: evohime.desktop.v1.GetIndexStatus.$Shape|null;
                  cancelWorkspaceIndex?: evohime.desktop.v1.CancelWorkspaceIndex.$Shape|null;
                  rotateReceiptKey?: evohime.desktop.v1.RotateReceiptKey.$Shape|null;
                  trustReceiptGenesis?: evohime.desktop.v1.TrustReceiptGenesis.$Shape|null;
                  getReceiptKeyStatus?: evohime.desktop.v1.GetReceiptKeyStatus.$Shape|null;
                  createNewReceiptGenesis?: evohime.desktop.v1.CreateNewReceiptGenesis.$Shape|null;
                  closePendingReceiptAction?: evohime.desktop.v1.ClosePendingReceiptAction.$Shape|null;
                  setReceiptAuditSamplingRate?: evohime.desktop.v1.SetReceiptAuditSamplingRate.$Shape|null;
                  reconcilePendingReceiptAction?: evohime.desktop.v1.ReconcilePendingReceiptAction.$Shape|null;
                  unquarantineReceiptAction?: evohime.desktop.v1.UnquarantineReceiptAction.$Shape|null;
                  listReceipts?: evohime.desktop.v1.ListReceipts.$Shape|null;
                  verifyReceipts?: evohime.desktop.v1.VerifyReceipts.$Shape|null;
                  exportReceipts?: evohime.desktop.v1.ExportReceipts.$Shape|null;
                  revisePlan?: evohime.desktop.v1.RevisePlan.$Shape|null;
                  stopRevision?: evohime.desktop.v1.StopRevision.$Shape|null;
                  saveRevisedPlan?: evohime.desktop.v1.SaveRevisedPlan.$Shape|null;
                  resolveRoutingDecision?: evohime.desktop.v1.ResolveRoutingDecision.$Shape|null;
                  setAmbientListening?: evohime.desktop.v1.SetAmbientListening.$Shape|null;
                  getAmbientStatus?: evohime.desktop.v1.GetAmbientStatus.$Shape|null;
                  listAmbientEpisodes?: evohime.desktop.v1.ListAmbientEpisodes.$Shape|null;
                  getAmbientEpisode?: evohime.desktop.v1.GetAmbientEpisode.$Shape|null;
                  deleteAmbientTranscripts?: evohime.desktop.v1.DeleteAmbientTranscripts.$Shape|null;
                  forgetAmbientWindow?: evohime.desktop.v1.ForgetAmbientWindow.$Shape|null;
                  getAmbientPolicy?: evohime.desktop.v1.GetAmbientPolicy.$Shape|null;
                  saveAmbientPolicy?: evohime.desktop.v1.SaveAmbientPolicy.$Shape|null;
                  resolveAmbientProposal?: evohime.desktop.v1.ResolveAmbientProposal.$Shape|null;
                  listAmbientProposals?: evohime.desktop.v1.ListAmbientProposals.$Shape|null;
                  listWorkflowTemplates?: evohime.desktop.v1.ListWorkflowTemplates.$Shape|null;
                  getWorkflowDefinition?: evohime.desktop.v1.GetWorkflowDefinition.$Shape|null;
                  startWorkflow?: evohime.desktop.v1.StartWorkflow.$Shape|null;
                  getWorkflowRun?: evohime.desktop.v1.GetWorkflowRun.$Shape|null;
                  cancelWorkflow?: evohime.desktop.v1.CancelWorkflow.$Shape|null;
                  listWorkflowEvents?: evohime.desktop.v1.ListWorkflowEvents.$Shape|null;
                  $unknowns?: Uint8Array[];
                } & (
                  ({ command?: undefined; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "handshake"; handshake: evohime.desktop.v1.Handshake.$Shape; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "replayEvents"; handshake?: null; replayEvents: evohime.desktop.v1.ReplayEvents.$Shape; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "startTask"; handshake?: null; replayEvents?: null; startTask: evohime.desktop.v1.StartTask.$Shape; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "stopTask"; handshake?: null; replayEvents?: null; startTask?: null; stopTask: evohime.desktop.v1.StopTask.$Shape; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "resolveApproval"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval: evohime.desktop.v1.ResolveApproval.$Shape; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "modelConfig"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig: evohime.desktop.v1.ModelConfigRequest.$Shape; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "modelCatalog"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog: evohime.desktop.v1.ModelCatalogRequest.$Shape; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "permissionMode"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode: evohime.desktop.v1.PermissionModeRequest.$Shape; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "createProject"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject: evohime.desktop.v1.CreateProject.$Shape; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "createTask"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask: evohime.desktop.v1.CreateTask.$Shape; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "updateTaskStatus"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus: evohime.desktop.v1.UpdateTaskStatus.$Shape; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "addTaskEdge"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge: evohime.desktop.v1.AddTaskEdge.$Shape; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getTaskGraph"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph: evohime.desktop.v1.GetTaskGraph.$Shape; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "nextReadyTask"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask: evohime.desktop.v1.NextReadyTask.$Shape; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "importPrd"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd: evohime.desktop.v1.ImportPrd.$Shape; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getTaskHistory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory: evohime.desktop.v1.GetTaskHistory.$Shape; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getTaskContext"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext: evohime.desktop.v1.GetTaskContext.$Shape; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getTaskPlanSpec"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec: evohime.desktop.v1.GetTaskPlanSpec.$Shape; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "applyApprovedBuild"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild: evohime.desktop.v1.ApplyApprovedBuild.$Shape; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "prepareBuild"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild: evohime.desktop.v1.PrepareBuild.$Shape; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getTaskSnapshot"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot: evohime.desktop.v1.GetTaskSnapshot.$Shape; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "restoreTaskSnapshot"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot: evohime.desktop.v1.RestoreTaskSnapshot.$Shape; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getBuildPolicy"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy: evohime.desktop.v1.GetBuildPolicy.$Shape; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "saveBuildPolicy"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy: evohime.desktop.v1.SaveBuildPolicy.$Shape; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "resyncRequest"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest: evohime.desktop.v1.ResyncRequest.$Shape; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "runDoctor"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor: evohime.desktop.v1.RunDoctor.$Shape; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "saveResearchEvidence"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence: evohime.desktop.v1.SaveResearchEvidence.$Shape; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "listResearchEvidence"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence: evohime.desktop.v1.ListResearchEvidence.$Shape; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "createMemory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory: evohime.desktop.v1.CreateMemory.$Shape; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "listMemory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory: evohime.desktop.v1.ListMemory.$Shape; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "searchMemory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory: evohime.desktop.v1.SearchMemory.$Shape; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "archiveMemory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory: evohime.desktop.v1.ArchiveMemory.$Shape; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "forgetMemory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory: evohime.desktop.v1.ForgetMemory.$Shape; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "installCapability"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability: evohime.desktop.v1.InstallCapability.$Shape; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "listCapabilities"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities: evohime.desktop.v1.ListCapabilities.$Shape; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "matchCapabilities"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities: evohime.desktop.v1.MatchCapabilities.$Shape; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "removeCapability"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability: evohime.desktop.v1.RemoveCapability.$Shape; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "requestChildHandoff"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff: evohime.desktop.v1.RequestChildHandoff.$Shape; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "listChildHandoffs"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs: evohime.desktop.v1.ListChildHandoffs.$Shape; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "submitChildRequest"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest: evohime.desktop.v1.SubmitChildRequest.$Shape; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "submitChildReport"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport: evohime.desktop.v1.SubmitChildReport.$Shape; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "runResearchFetch"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch: evohime.desktop.v1.RunResearchFetch.$Shape; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "listWorkspace"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace: evohime.desktop.v1.ListWorkspace.$Shape; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "readWorkspaceFile"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile: evohime.desktop.v1.ReadWorkspaceFile.$Shape; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "gitStatus"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus: evohime.desktop.v1.GitStatus.$Shape; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "gitDiff"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff: evohime.desktop.v1.GitDiff.$Shape; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "terminalExecute"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute: evohime.desktop.v1.TerminalExecute.$Shape; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "exportDoctorLogs"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs: evohime.desktop.v1.ExportDoctorLogs.$Shape; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getCapabilitySelection"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection: evohime.desktop.v1.GetCapabilitySelection.$Shape; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "pinCapabilitySelection"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection: evohime.desktop.v1.PinCapabilitySelection.$Shape; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "replaceCapabilitySelection"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection: evohime.desktop.v1.ReplaceCapabilitySelection.$Shape; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "submitFeedback"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback: evohime.desktop.v1.SubmitFeedback.$Shape; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "listFeedback"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback: evohime.desktop.v1.ListFeedback.$Shape; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "createDatabaseBackup"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup: evohime.desktop.v1.CreateDatabaseBackup.$Shape; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "prepareDatabaseRestore"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore: evohime.desktop.v1.PrepareDatabaseRestore.$Shape; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "restoreDatabase"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase: evohime.desktop.v1.RestoreDatabase.$Shape; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "selectModel"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel: evohime.desktop.v1.SelectModelRequest.$Shape; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "cancelDatabaseOperation"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation: evohime.desktop.v1.CancelDatabaseOperation.$Shape; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getMemory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory: evohime.desktop.v1.GetMemory.$Shape; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "listMemoryPending"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending: evohime.desktop.v1.ListMemoryPending.$Shape; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getMemoryConflicts"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts: evohime.desktop.v1.GetMemoryConflicts.$Shape; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "confirmMemory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory: evohime.desktop.v1.ConfirmMemory.$Shape; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "rejectMemory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory: evohime.desktop.v1.RejectMemory.$Shape; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "supersedeMemory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory: evohime.desktop.v1.SupersedeMemory.$Shape; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "reviseMemoryCandidate"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate: evohime.desktop.v1.ReviseMemoryCandidate.$Shape; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "startPlanReview"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview: evohime.desktop.v1.StartPlanReview.$Shape; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "stopPlanReview"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview: evohime.desktop.v1.StopPlanReview.$Shape; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "listPlanReviews"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews: evohime.desktop.v1.ListPlanReviews.$Shape; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getPlanReview"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview: evohime.desktop.v1.GetPlanReview.$Shape; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "exportPlanReview"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview: evohime.desktop.v1.ExportPlanReview.$Shape; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "clearPlanReviewHistory"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory: evohime.desktop.v1.ClearPlanReviewHistory.$Shape; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getContextLedger"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger: evohime.desktop.v1.GetContextLedger.$Shape; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "listTaskScratchpad"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad: evohime.desktop.v1.ListTaskScratchpad.$Shape; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "clearTaskScratchpad"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad: evohime.desktop.v1.ClearTaskScratchpad.$Shape; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "summarizeContextNow"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow: evohime.desktop.v1.SummarizeContextNow.$Shape; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "pinContextItem"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem: evohime.desktop.v1.PinContextItem.$Shape; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "readContextArtifact"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact: evohime.desktop.v1.ReadContextArtifact.$Shape; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "indexWorkspace"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace: evohime.desktop.v1.IndexWorkspace.$Shape; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "rebuildIndex"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex: evohime.desktop.v1.RebuildIndex.$Shape; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "searchWorkspaceKnowledge"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge: evohime.desktop.v1.SearchWorkspaceKnowledge.$Shape; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getIndexStatus"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus: evohime.desktop.v1.GetIndexStatus.$Shape; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "cancelWorkspaceIndex"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex: evohime.desktop.v1.CancelWorkspaceIndex.$Shape; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "rotateReceiptKey"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey: evohime.desktop.v1.RotateReceiptKey.$Shape; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "trustReceiptGenesis"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis: evohime.desktop.v1.TrustReceiptGenesis.$Shape; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getReceiptKeyStatus"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus: evohime.desktop.v1.GetReceiptKeyStatus.$Shape; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "createNewReceiptGenesis"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis: evohime.desktop.v1.CreateNewReceiptGenesis.$Shape; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "closePendingReceiptAction"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction: evohime.desktop.v1.ClosePendingReceiptAction.$Shape; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "setReceiptAuditSamplingRate"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate: evohime.desktop.v1.SetReceiptAuditSamplingRate.$Shape; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "reconcilePendingReceiptAction"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction: evohime.desktop.v1.ReconcilePendingReceiptAction.$Shape; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "unquarantineReceiptAction"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction: evohime.desktop.v1.UnquarantineReceiptAction.$Shape; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "listReceipts"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts: evohime.desktop.v1.ListReceipts.$Shape; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "verifyReceipts"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts: evohime.desktop.v1.VerifyReceipts.$Shape; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "exportReceipts"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts: evohime.desktop.v1.ExportReceipts.$Shape; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "revisePlan"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan: evohime.desktop.v1.RevisePlan.$Shape; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "stopRevision"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision: evohime.desktop.v1.StopRevision.$Shape; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "saveRevisedPlan"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan: evohime.desktop.v1.SaveRevisedPlan.$Shape; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "resolveRoutingDecision"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision: evohime.desktop.v1.ResolveRoutingDecision.$Shape; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "setAmbientListening"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening: evohime.desktop.v1.SetAmbientListening.$Shape; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getAmbientStatus"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus: evohime.desktop.v1.GetAmbientStatus.$Shape; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "listAmbientEpisodes"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes: evohime.desktop.v1.ListAmbientEpisodes.$Shape; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getAmbientEpisode"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode: evohime.desktop.v1.GetAmbientEpisode.$Shape; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "deleteAmbientTranscripts"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts: evohime.desktop.v1.DeleteAmbientTranscripts.$Shape; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "forgetAmbientWindow"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow: evohime.desktop.v1.ForgetAmbientWindow.$Shape; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getAmbientPolicy"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy: evohime.desktop.v1.GetAmbientPolicy.$Shape; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "saveAmbientPolicy"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy: evohime.desktop.v1.SaveAmbientPolicy.$Shape; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "resolveAmbientProposal"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal: evohime.desktop.v1.ResolveAmbientProposal.$Shape; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "listAmbientProposals"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals: evohime.desktop.v1.ListAmbientProposals.$Shape; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "listWorkflowTemplates"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates: evohime.desktop.v1.ListWorkflowTemplates.$Shape; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getWorkflowDefinition"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition: evohime.desktop.v1.GetWorkflowDefinition.$Shape; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "startWorkflow"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow: evohime.desktop.v1.StartWorkflow.$Shape; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "getWorkflowRun"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun: evohime.desktop.v1.GetWorkflowRun.$Shape; cancelWorkflow?: null; listWorkflowEvents?: null }|{ command?: "cancelWorkflow"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow: evohime.desktop.v1.CancelWorkflow.$Shape; listWorkflowEvents?: null }|{ command?: "listWorkflowEvents"; handshake?: null; replayEvents?: null; startTask?: null; stopTask?: null; resolveApproval?: null; modelConfig?: null; modelCatalog?: null; permissionMode?: null; createProject?: null; createTask?: null; updateTaskStatus?: null; addTaskEdge?: null; getTaskGraph?: null; nextReadyTask?: null; importPrd?: null; getTaskHistory?: null; getTaskContext?: null; getTaskPlanSpec?: null; applyApprovedBuild?: null; prepareBuild?: null; getTaskSnapshot?: null; restoreTaskSnapshot?: null; getBuildPolicy?: null; saveBuildPolicy?: null; resyncRequest?: null; runDoctor?: null; saveResearchEvidence?: null; listResearchEvidence?: null; createMemory?: null; listMemory?: null; searchMemory?: null; archiveMemory?: null; forgetMemory?: null; installCapability?: null; listCapabilities?: null; matchCapabilities?: null; removeCapability?: null; requestChildHandoff?: null; listChildHandoffs?: null; submitChildRequest?: null; submitChildReport?: null; runResearchFetch?: null; listWorkspace?: null; readWorkspaceFile?: null; gitStatus?: null; gitDiff?: null; terminalExecute?: null; exportDoctorLogs?: null; getCapabilitySelection?: null; pinCapabilitySelection?: null; replaceCapabilitySelection?: null; submitFeedback?: null; listFeedback?: null; createDatabaseBackup?: null; prepareDatabaseRestore?: null; restoreDatabase?: null; selectModel?: null; cancelDatabaseOperation?: null; getMemory?: null; listMemoryPending?: null; getMemoryConflicts?: null; confirmMemory?: null; rejectMemory?: null; supersedeMemory?: null; reviseMemoryCandidate?: null; startPlanReview?: null; stopPlanReview?: null; listPlanReviews?: null; getPlanReview?: null; exportPlanReview?: null; clearPlanReviewHistory?: null; getContextLedger?: null; listTaskScratchpad?: null; clearTaskScratchpad?: null; summarizeContextNow?: null; pinContextItem?: null; readContextArtifact?: null; indexWorkspace?: null; rebuildIndex?: null; searchWorkspaceKnowledge?: null; getIndexStatus?: null; cancelWorkspaceIndex?: null; rotateReceiptKey?: null; trustReceiptGenesis?: null; getReceiptKeyStatus?: null; createNewReceiptGenesis?: null; closePendingReceiptAction?: null; setReceiptAuditSamplingRate?: null; reconcilePendingReceiptAction?: null; unquarantineReceiptAction?: null; listReceipts?: null; verifyReceipts?: null; exportReceipts?: null; revisePlan?: null; stopRevision?: null; saveRevisedPlan?: null; resolveRoutingDecision?: null; setAmbientListening?: null; getAmbientStatus?: null; listAmbientEpisodes?: null; getAmbientEpisode?: null; deleteAmbientTranscripts?: null; forgetAmbientWindow?: null; getAmbientPolicy?: null; saveAmbientPolicy?: null; resolveAmbientProposal?: null; listAmbientProposals?: null; listWorkflowTemplates?: null; getWorkflowDefinition?: null; startWorkflow?: null; getWorkflowRun?: null; cancelWorkflow?: null; listWorkflowEvents: evohime.desktop.v1.ListWorkflowEvents.$Shape })
                );
            }

            /**
             * Properties of a Ready.
             * @deprecated Use evohime.desktop.v1.Ready.$Properties instead.
             */
            interface IReady extends evohime.desktop.v1.Ready.$Properties {
            }

            /** Represents a Ready. */
            class Ready {

                /**
                 * Constructs a new Ready.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.Ready.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** Ready protocol. */
                protocol?: (evohime.desktop.v1.ProtocolVersion.$Properties|null);

                /** Ready coreVersion. */
                coreVersion: string;

                /**
                 * Encodes the specified Ready message. Does not implicitly {@link evohime.desktop.v1.Ready.verify|verify} messages.
                 * @param message Ready message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.Ready.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a Ready message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.Ready & evohime.desktop.v1.Ready.$Shape} Ready
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.Ready & evohime.desktop.v1.Ready.$Shape;

                /**
                 * Gets the type url for Ready
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace Ready {

                /** Properties of a Ready. */
                interface $Properties {

                    /** Ready protocol */
                    protocol?: (evohime.desktop.v1.ProtocolVersion.$Properties|null);

                    /** Ready coreVersion */
                    coreVersion?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a Ready. */
                type $Shape = evohime.desktop.v1.Ready.$Properties;
            }

            /**
             * Properties of an EventEnvelope.
             * @deprecated Use evohime.desktop.v1.EventEnvelope.$Properties instead.
             */
            interface IEventEnvelope extends evohime.desktop.v1.EventEnvelope.$Properties {
            }

            /** Represents an EventEnvelope. */
            class EventEnvelope {

                /**
                 * Constructs a new EventEnvelope.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.EventEnvelope.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** EventEnvelope protocol. */
                protocol?: (evohime.desktop.v1.ProtocolVersion.$Properties|null);

                /** EventEnvelope sequenceId. */
                sequenceId: number;

                /** EventEnvelope taskId. */
                taskId: string;

                /** EventEnvelope eventType. */
                eventType: string;

                /** EventEnvelope payload. */
                payload: Uint8Array;

                /** EventEnvelope coreInstanceId. */
                coreInstanceId: string;

                /** EventEnvelope sessionEpoch. */
                sessionEpoch: number;

                /** EventEnvelope ready. */
                ready?: (evohime.desktop.v1.Ready.$Properties|null);

                /** EventEnvelope replayGap. */
                replayGap?: (evohime.desktop.v1.ReplayGap.$Properties|null);

                /** EventEnvelope fullSnapshot. */
                fullSnapshot?: (evohime.desktop.v1.FullSnapshot.$Properties|null);

                /** EventEnvelope authChallenge. */
                authChallenge?: (evohime.desktop.v1.AuthChallenge.$Properties|null);

                /** EventEnvelope event. */
                event?: ("ready"|"replayGap"|"fullSnapshot"|"authChallenge");

                /**
                 * Encodes the specified EventEnvelope message. Does not implicitly {@link evohime.desktop.v1.EventEnvelope.verify|verify} messages.
                 * @param message EventEnvelope message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.EventEnvelope.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an EventEnvelope message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.EventEnvelope & evohime.desktop.v1.EventEnvelope.$Shape} EventEnvelope
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.EventEnvelope & evohime.desktop.v1.EventEnvelope.$Shape;

                /**
                 * Gets the type url for EventEnvelope
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace EventEnvelope {

                /** Properties of an EventEnvelope. */
                interface $Properties {

                    /** EventEnvelope protocol */
                    protocol?: (evohime.desktop.v1.ProtocolVersion.$Properties|null);

                    /** EventEnvelope sequenceId */
                    sequenceId?: (number|null);

                    /** EventEnvelope taskId */
                    taskId?: (string|null);

                    /** EventEnvelope eventType */
                    eventType?: (string|null);

                    /** EventEnvelope payload */
                    payload?: (Uint8Array|null);

                    /** EventEnvelope coreInstanceId */
                    coreInstanceId?: (string|null);

                    /** EventEnvelope sessionEpoch */
                    sessionEpoch?: (number|null);

                    /** EventEnvelope ready */
                    ready?: (evohime.desktop.v1.Ready.$Properties|null);

                    /** EventEnvelope replayGap */
                    replayGap?: (evohime.desktop.v1.ReplayGap.$Properties|null);

                    /** EventEnvelope fullSnapshot */
                    fullSnapshot?: (evohime.desktop.v1.FullSnapshot.$Properties|null);

                    /** EventEnvelope authChallenge */
                    authChallenge?: (evohime.desktop.v1.AuthChallenge.$Properties|null);

                    /** EventEnvelope event */
                    event?: ("ready"|"replayGap"|"fullSnapshot"|"authChallenge");

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Narrowed shape of an EventEnvelope. */
                type $Shape = {
                  protocol?: evohime.desktop.v1.ProtocolVersion.$Shape|null;
                  sequenceId?: number|null;
                  taskId?: string|null;
                  eventType?: string|null;
                  payload?: Uint8Array|null;
                  coreInstanceId?: string|null;
                  sessionEpoch?: number|null;
                  ready?: evohime.desktop.v1.Ready.$Shape|null;
                  replayGap?: evohime.desktop.v1.ReplayGap.$Shape|null;
                  fullSnapshot?: evohime.desktop.v1.FullSnapshot.$Shape|null;
                  authChallenge?: evohime.desktop.v1.AuthChallenge.$Shape|null;
                  $unknowns?: Uint8Array[];
                } & (
                  ({ event?: undefined; ready?: null; replayGap?: null; fullSnapshot?: null; authChallenge?: null }|{ event?: "ready"; ready: evohime.desktop.v1.Ready.$Shape; replayGap?: null; fullSnapshot?: null; authChallenge?: null }|{ event?: "replayGap"; ready?: null; replayGap: evohime.desktop.v1.ReplayGap.$Shape; fullSnapshot?: null; authChallenge?: null }|{ event?: "fullSnapshot"; ready?: null; replayGap?: null; fullSnapshot: evohime.desktop.v1.FullSnapshot.$Shape; authChallenge?: null }|{ event?: "authChallenge"; ready?: null; replayGap?: null; fullSnapshot?: null; authChallenge: evohime.desktop.v1.AuthChallenge.$Shape })
                );
            }

            /**
             * Properties of a ReplayGap.
             * @deprecated Use evohime.desktop.v1.ReplayGap.$Properties instead.
             */
            interface IReplayGap extends evohime.desktop.v1.ReplayGap.$Properties {
            }

            /** Represents a ReplayGap. */
            class ReplayGap {

                /**
                 * Constructs a new ReplayGap.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.ReplayGap.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** ReplayGap requestedAfterSequence. */
                requestedAfterSequence: number;

                /** ReplayGap earliestAvailableSequence. */
                earliestAvailableSequence: number;

                /** ReplayGap latestAvailableSequence. */
                latestAvailableSequence: number;

                /** ReplayGap reason. */
                reason: string;

                /**
                 * Encodes the specified ReplayGap message. Does not implicitly {@link evohime.desktop.v1.ReplayGap.verify|verify} messages.
                 * @param message ReplayGap message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.ReplayGap.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a ReplayGap message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.ReplayGap & evohime.desktop.v1.ReplayGap.$Shape} ReplayGap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.ReplayGap & evohime.desktop.v1.ReplayGap.$Shape;

                /**
                 * Gets the type url for ReplayGap
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace ReplayGap {

                /** Properties of a ReplayGap. */
                interface $Properties {

                    /** ReplayGap requestedAfterSequence */
                    requestedAfterSequence?: (number|null);

                    /** ReplayGap earliestAvailableSequence */
                    earliestAvailableSequence?: (number|null);

                    /** ReplayGap latestAvailableSequence */
                    latestAvailableSequence?: (number|null);

                    /** ReplayGap reason */
                    reason?: (string|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a ReplayGap. */
                type $Shape = evohime.desktop.v1.ReplayGap.$Properties;
            }

            /**
             * Properties of a FullSnapshot.
             * @deprecated Use evohime.desktop.v1.FullSnapshot.$Properties instead.
             */
            interface IFullSnapshot extends evohime.desktop.v1.FullSnapshot.$Properties {
            }

            /** Represents a FullSnapshot. */
            class FullSnapshot {

                /**
                 * Constructs a new FullSnapshot.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: evohime.desktop.v1.FullSnapshot.$Properties);

                /** Unknown fields preserved while decoding when enabled */
                $unknowns?: Uint8Array[];

                /** FullSnapshot sequenceId. */
                sequenceId: number;

                /** FullSnapshot snapshotJson. */
                snapshotJson: Uint8Array;

                /**
                 * Encodes the specified FullSnapshot message. Does not implicitly {@link evohime.desktop.v1.FullSnapshot.verify|verify} messages.
                 * @param message FullSnapshot message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                static encode(message: evohime.desktop.v1.FullSnapshot.$Properties, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a FullSnapshot message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns {evohime.desktop.v1.FullSnapshot & evohime.desktop.v1.FullSnapshot.$Shape} FullSnapshot
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): evohime.desktop.v1.FullSnapshot & evohime.desktop.v1.FullSnapshot.$Shape;

                /**
                 * Gets the type url for FullSnapshot
                 * @param [prefix] Custom type url prefix, defaults to `"type.googleapis.com"`
                 * @returns The type url
                 */
                static getTypeUrl(prefix?: string): string;
            }

            namespace FullSnapshot {

                /** Properties of a FullSnapshot. */
                interface $Properties {

                    /** FullSnapshot sequenceId */
                    sequenceId?: (number|null);

                    /** FullSnapshot snapshotJson */
                    snapshotJson?: (Uint8Array|null);

                    /** Unknown fields preserved while decoding when enabled */
                    $unknowns?: Uint8Array[];
                }

                /** Shape of a FullSnapshot. */
                type $Shape = evohime.desktop.v1.FullSnapshot.$Properties;
            }
        }
    }
}
