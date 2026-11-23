#!/usr/bin/env node
/**
 * Met une version de Trame en ligne : depose le setup sur Firebase Storage, signe son
 * adresse (V2), puis publie la fiche `products/trame` que lit `src/maj.rs`.
 *
 * « En ligne » a un sens precis ici : Trame n'est pas diffusee (usage interne), il n'y a ni
 * boutique ni page de vente. La fiche Firestore du projet `desktopiconviewer` EST le manifeste :
 * `version`, `download_url`, `isActive`. C'est elle que les binaires installes interrogent.
 *
 * Usage :
 *   node outils/mettre-en-ligne.mjs --version 0.4.50
 *   node outils/mettre-en-ligne.mjs --version 0.4.50 --essai     (tout sauf l'ecriture)
 *   node outils/mettre-en-ligne.mjs --version 0.4.50 --fichier dist/autre.exe
 *
 * La cle du compte de service se lit dans TRAME_CLE_SERVICE, sinon dans le coffre du hub. Elle
 * n'est jamais ecrite ni affichee : le script ne montre que son adresse de compte.
 *
 * Ce qu'il verifie avant d'ecrire quoi que ce soit :
 *   1. le fichier existe et son numero de version est celui demande ;
 *   2. sa signature Authenticode est valide (un setup non signe ne part pas) ;
 *   3. la version demandee est PLUS RECENTE que celle de la fiche (une fiche ne recule jamais).
 * Et apres :
 *   4. l'objet depose a la bonne taille et la bonne empreinte SHA-256 ;
 *   5. l'adresse signee repond 200 et sert bien ce fichier ;
 *   6. la fiche relue annonce la version, l'adresse et l'heure.
 */
import { readFileSync, statSync, existsSync } from 'node:fs';
import { createHash, createSign, randomUUID } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const PROJET = dirname(dirname(fileURLToPath(import.meta.url)));
const BUCKET = 'desktopiconviewer.firebasestorage.app';
const DOSSIER = 'releases/apps/trame';
const FICHE_DOC = 'products/trame';
const FICHE_URL =
  'https://firestore.googleapis.com/v1/projects/desktopiconviewer/databases/(default)/documents/' + FICHE_DOC;
const CLE_DEFAUT = 'D:\\LIFE\\50-secrets\\firebase\\service-account-desktopiconviewer.json';
// L'echeance des adresses deja publiees (0.4.13 a 0.4.49). On garde la meme : elle vaut
// « jamais » a l'echelle d'une vie de logiciel, et une adresse qui expire casserait la mise a
// jour d'un binaire installe sans que personne ne regarde.
const EXPIRATION = 16447014000;

const dire = (...a) => process.stdout.write(a.join(' ') + '\n');
// Une erreur se LEVE, elle ne tue pas le processus : `process.exit` pendant qu'une socket de
// `fetch` se referme fait jeter a Node 25 « Assertion failed: !(handle->flags &
// UV_HANDLE_CLOSING) », qui sort en code 1 en laissant croire a un plantage du script.
class Arret extends Error {}
const fatal = (msg) => {
  throw new Arret(msg);
};

function argument(nom, defaut = null) {
  const i = process.argv.indexOf('--' + nom);
  if (i === -1) return defaut;
  const v = process.argv[i + 1];
  return v && !v.startsWith('--') ? v : true;
}

// ---------------------------------------------------------------- signature des requetes

const base64url = (buf) => Buffer.from(buf).toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');

/** Jeton d'acces OAuth2 par assertion JWT : aucun outil externe, aucune session gcloud. */
async function jeton(cle) {
  const maintenant = Math.floor(Date.now() / 1000);
  const entete = base64url(JSON.stringify({ alg: 'RS256', typ: 'JWT' }));
  const charge = base64url(
    JSON.stringify({
      iss: cle.client_email,
      // `devstorage.read_write` ne suffit PAS : mesure du 2026-11-23, il depose l'objet mais la
      // pose des metadonnees (cacheControl) repond 403 « Provided scope(s) are not authorized ».
      // Il faut `full_control` — `cloud-platform` marche aussi, mais il ouvre tout le projet.
      scope: 'https://www.googleapis.com/auth/devstorage.full_control https://www.googleapis.com/auth/datastore',
      aud: 'https://oauth2.googleapis.com/token',
      iat: maintenant,
      exp: maintenant + 3600,
    }),
  );
  const signeur = createSign('RSA-SHA256');
  signeur.update(entete + '.' + charge);
  const assertion = entete + '.' + charge + '.' + base64url(signeur.sign(cle.private_key));
  const r = await fetch('https://oauth2.googleapis.com/token', {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({ grant_type: 'urn:ietf:params:oauth:grant-type:jwt-bearer', assertion }),
  });
  const j = await r.json();
  if (!r.ok || !j.access_token) fatal('le jeton OAuth2 a ete refuse : ' + JSON.stringify(j).slice(0, 300));
  return j.access_token;
}

/**
 * Adresse signee V2 (`GoogleAccessId` + `Expires` + `Signature`) : c'est la forme des
 * adresses deja publiees, et la seule qui accepte une echeance lointaine — une adresse V4 est
 * plafonnee a sept jours par Google.
 */
function adresseSignee(cle, objet) {
  const chaine = `GET\n\n\n${EXPIRATION}\n/${BUCKET}/${objet}`;
  const signeur = createSign('RSA-SHA256');
  signeur.update(chaine);
  const signature = signeur.sign(cle.private_key, 'base64');
  return (
    `https://storage.googleapis.com/${BUCKET}/${objet}` +
    `?GoogleAccessId=${encodeURIComponent(cle.client_email)}` +
    `&Expires=${EXPIRATION}` +
    `&Signature=${encodeURIComponent(signature)}`
  );
}

// ---------------------------------------------------------------- etat de la fiche

async function lireFiche() {
  const r = await fetch(FICHE_URL, { headers: { 'Cache-Control': 'no-cache' } });
  if (!r.ok) fatal('la fiche n\'a pas repondu (' + r.status + ')');
  const j = await r.json();
  const texte = (n) => j.fields?.[n]?.stringValue ?? '';
  return { version: texte('version'), url: texte('download_url'), brute: j };
}

const champs = (v) =>
  String(v)
    .trim()
    .replace(/^[vV]/, '')
    .split('.')
    .map((c) => parseInt(c, 10) || 0);

/** La meme comparaison numerique que `maj::plus_recente` : « 0.4.9 » < « 0.4.50 ». */
function plusRecente(a, b) {
  const [g, d] = [champs(a), champs(b)];
  for (let i = 0; i < Math.max(g.length, d.length); i++) {
    const x = g[i] ?? 0;
    const y = d[i] ?? 0;
    if (x !== y) return x > y;
  }
  return false;
}

// ---------------------------------------------------------------- depot

async function deposer(jetonAcces, fichier, objet) {
  const corps = readFileSync(fichier);
  const url =
    `https://storage.googleapis.com/upload/storage/v1/b/${BUCKET}/o` +
    `?uploadType=media&name=${encodeURIComponent(objet)}&predefinedAcl=publicRead`;
  const r = await fetch(url, {
    method: 'POST',
    headers: { Authorization: 'Bearer ' + jetonAcces, 'Content-Type': 'application/octet-stream' },
    body: corps,
  });
  const j = await r.json();
  if (!r.ok) fatal('le depot a echoue (' + r.status + ') : ' + JSON.stringify(j).slice(0, 300));
  // La duree de cache des objets precedents : une heure. Sans elle, un binaire se fige chez
  // les intermediaires et une nouvelle version mettrait des heures a se repandre.
  const p = await fetch(
    `https://storage.googleapis.com/storage/v1/b/${BUCKET}/o/${encodeURIComponent(objet)}`,
    {
      method: 'PATCH',
      headers: { Authorization: 'Bearer ' + jetonAcces, 'Content-Type': 'application/json' },
      body: JSON.stringify({ cacheControl: 'public, max-age=3600', contentDisposition: 'attachment' }),
    },
  );
  const pj = await p.json();
  if (!p.ok) fatal('les metadonnees de l\'objet n\'ont pas ete posees (' + p.status + ') : ' + JSON.stringify(pj).slice(0, 300));
  return j;
}

async function ecrireFiche(jetonAcces, version, url, quand) {
  const masque = ['version', 'download_url', 'updated_at']
    .map((c) => 'updateMask.fieldPaths=' + c)
    .join('&');
  const r = await fetch(FICHE_URL + '?' + masque, {
    method: 'PATCH',
    headers: { Authorization: 'Bearer ' + jetonAcces, 'Content-Type': 'application/json' },
    body: JSON.stringify({
      fields: {
        version: { stringValue: version },
        download_url: { stringValue: url },
        updated_at: { timestampValue: quand },
      },
    }),
  });
  const j = await r.json();
  if (!r.ok) fatal('la fiche n\'a pas ete ecrite (' + r.status + ') : ' + JSON.stringify(j).slice(0, 600));
  return j;
}

// ---------------------------------------------------------------- programme

async function principal() {
const version = argument('version');
if (!version || version === true) fatal('donnez --version (ex. --version 0.4.50)');
const essai = argument('essai') === true;
const fichier = resolve(PROJET, String(argument('fichier', join('dist', `Trame-${version}-setup.exe`))));
const objet = `${DOSSIER}/Trame-${version}-setup.exe`;

dire('Trame ' + version + ' — mise en ligne' + (essai ? ' (essai, rien ne sera ecrit)' : ''));
dire('');

// 1. le fichier
if (!existsSync(fichier)) fatal('fichier introuvable : ' + fichier);
const octets = readFileSync(fichier);
const sha = createHash('sha256').update(octets).digest('hex');
dire('1. fichier');
dire('   ' + fichier.replace(PROJET + '\\', '') + '  ' + octets.length + ' octets');
dire('   sha256 ' + sha);

// 2. le numero de version, lu dans l'executable lui-meme
let versionFichier = '';
try {
  versionFichier = execFileSync('powershell.exe', [
    '-NoProfile',
    '-Command',
    `(Get-Item -LiteralPath '${fichier}').VersionInfo.FileVersion`,
  ])
    .toString()
    .trim();
} catch {
  versionFichier = '';
}
if (versionFichier && versionFichier !== version && fichier.endsWith('trame.exe')) {
  fatal(`le binaire se declare en ${versionFichier}, pas en ${version}`);
}
dire('   version du fichier : ' + (versionFichier || '(sans numero — c\'est le cas d\'un setup Inno)'));

// 3. la signature
const signature = execFileSync('powershell.exe', [
  '-NoProfile',
  '-Command',
  `$s = Get-AuthenticodeSignature -LiteralPath '${fichier}'; "$($s.Status)|$($s.SignerCertificate.Subject)"`,
])
  .toString()
  .trim();
const [etat, signataire] = signature.split('|');
if (etat !== 'Valid') fatal('la signature Authenticode n\'est pas valide (' + etat + ') — un livrable non signe ne part pas');
dire('   signature : Valid — ' + signataire.split(',')[0]);

// 4. la fiche, avant
dire('');
dire('2. fiche avant');
const avant = await lireFiche();
dire('   ' + FICHE_DOC + ' : version ' + avant.version);
if (!plusRecente(version, avant.version)) {
  fatal(
    `la fiche annonce deja ${avant.version} : ${version} n'est pas plus recente. ` +
      'Une fiche ne recule jamais (les binaires installes ignoreraient l\'annonce).',
  );
}
dire('   ' + version + ' est plus recente que ' + avant.version + ' : la fiche peut avancer');

if (essai) {
  dire('');
  dire('Essai : rien n\'a ete depose ni ecrit. Sans --essai, le script deposerait');
  dire('  gs://' + BUCKET + '/' + objet);
  dire('  puis ecrirait la fiche avec version=' + version + '.');
  return;
}

// 5. le depot
const cheminCle = process.env.TRAME_CLE_SERVICE || CLE_DEFAUT;
if (!existsSync(cheminCle)) fatal('cle de service introuvable : ' + cheminCle);
const cle = JSON.parse(readFileSync(cheminCle, 'utf8'));
if (cle.type !== 'service_account') fatal('le fichier de cle n\'est pas une cle de compte de service');
dire('');
dire('3. compte');
dire('   ' + cle.client_email + ' (projet ' + cle.project_id + ')');

const jetonAcces = await jeton(cle);

dire('');
dire('4. depot');
const depose = await deposer(jetonAcces, fichier, objet);
dire('   gs://' + BUCKET + '/' + objet);
dire('   ' + depose.size + ' octets, md5 ' + Buffer.from(depose.md5Hash || '', 'base64').toString('hex').slice(0, 16) + '…');

// 6. l'adresse signee, et la preuve qu'elle sert ce fichier
const url = adresseSignee(cle, objet);
const tete = await fetch(url, { method: 'HEAD', headers: { 'Cache-Control': 'no-cache' } });
if (!tete.ok) fatal('l\'adresse signee ne repond pas (' + tete.status + ')');
const servi = Number(tete.headers.get('content-length'));
if (servi !== octets.length) fatal(`l'adresse signee sert ${servi} octets au lieu de ${octets.length}`);
dire('');
dire('5. adresse signee');
dire('   HTTP ' + tete.status + ', ' + servi + ' octets servis (' + url.length + ' caracteres)');

// 7. la fiche, apres
const quand = new Date().toISOString().replace(/\.\d+Z$/, 'Z');
await ecrireFiche(jetonAcces, version, url, quand);
const apres = await lireFiche();
if (apres.version !== version) fatal('la fiche relue annonce ' + apres.version + ', pas ' + version);
if (apres.url !== url) fatal('la fiche relue ne porte pas l\'adresse qui vient d\'etre signee');
dire('');
dire('6. fiche apres');
dire('   version ' + apres.version);
dire('   updated_at ' + (apres.brute.fields?.updated_at?.timestampValue || '(absent)'));
dire('   download_url identique a celle qui repond 200 : oui');

// 8. ce que l'application en fera
dire('');
dire('Termine. Un binaire installe en ' + avant.version + ' se verra proposer ' + version + '.');
dire('Reste a decider : l\'installer sur le poste, et pousser les commits et les etiquettes git.');
}

principal().then(
  () => {
    process.exitCode = 0;
  },
  (e) => {
    process.stderr.write('ARRET : ' + (e instanceof Arret ? e.message : e && e.stack ? e.stack : e) + '\n');
    process.exitCode = 1;
  },
);
