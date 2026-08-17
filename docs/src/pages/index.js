import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';
import styles from './index.module.css';

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <Heading as="h1" className="hero__title">
          {siteConfig.title}
        </Heading>
        <p className="hero__subtitle">{siteConfig.tagline}</p>
        <div className={styles.buttons}>
          <Link className="button button--secondary button--lg" to="/docs/intro">
            Read the documentation
          </Link>
        </div>
      </div>
    </header>
  );
}

export default function Home() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout
      title={siteConfig.title}
      description="Renvor is a Rust framework. It is pre-release, unpublished, and has no transport adapter yet.">
      <HomepageHeader />
      <main>
        <div className="container margin-vert--lg">
          <div className="row">
            <div className="col col--8 col--offset-2">
              <h2>Renvor cannot serve a request yet</h2>
              <p>
                <strong>Nothing is published</strong>, so there is no way to install
                Renvor. What exists today is a working transport-independent kernel:
                a seven-phase application lifecycle, provider dependency resolution,
                layered configuration with secret redaction, enforced deadlines on
                every call into your code, and independent liveness and readiness.
              </p>
              <p>
                It has <strong>no transport</strong> — no HTTP server, no database
                adapter, no CLI. You can start and stop an application; you cannot
                serve anything with one.
              </p>
              <p>
                <strong>Do not adopt Renvor for anything yet.</strong> Nothing is stable,
                and no compatibility promise applies before a <code>0.1.0</code> release.
              </p>
              <h2>The command is <code>renover</code></h2>
              <p>
                The product is <strong>Renvor</strong>. The installed executable is{' '}
                <strong><code>renover</code></strong>. The difference is deliberate and
                permanent — it is not a typographical error.
              </p>
            </div>
          </div>
        </div>
      </main>
    </Layout>
  );
}
